//! **The clock this host can sustain**, and the one place that converts a measured speed into one.
//!
//! [`CLOCK`](crate::CLOCK) is 75 because the PP5021C is 75 MHz. That is what the part *is*, and
//! every recipe in `research/` runs at it or says which other number it ran at. It is not, on any
//! machine this program has been measured on, what the host can *deliver*: a bare `trace` of a
//! retail boot sustains about 15 million interpreter instructions per second of wall time, and at
//! `--clock=75` one simulated second costs 75 million. So simulated time advances at about a fifth
//! of real time, and everything the firmware does — the list scroll, the menu animation, the wheel
//! poll — takes five times as long as it did on the part.
//!
//! **Slow motion is a thing no iPod has ever done.** A slower iPod is: run the same firmware on a
//! part clocked at 15 MHz and it drops frames, which real hardware does and people have seen. So
//! where the host cannot keep up, the honest answer is to say the part is slower, not to say that
//! time is. That is the whole of this module: pick the clock at which simulated time and wall time
//! agree, and hold it.
//!
//! # Why the arithmetic is a division and nothing else
//!
//! The simulated clock is `executed / instr_per_usec + slept_usec`, and a halted cycle costs the
//! same as an executed one (see [`Machine::idle_steps`](crate::Machine::idle_steps)). So one
//! *step* — instruction or halt — buys `1 / instr_per_usec` microseconds of the iPod's time,
//! whatever the machine is doing. If the host retires `S` steps in `W` seconds, then setting
//!
//! ```text
//! instr_per_usec = S / W / 1_000_000
//! ```
//!
//! makes one simulated second cost exactly one wall second. There is no fudge factor and no
//! constant to calibrate against, because the clock in force cancels out of the measurement: a run
//! at 75 that manages 15 M steps/s and a run at 15 that manages the same 15 M steps/s both name 15.
//!
//! **That fixed point is the reason this can be measured once and held.** Recalibrating a machine
//! that is already running at the right clock yields the number it already had.
//!
//! # What this is not
//!
//! It is not the fix. The fix is closing the gap so the host retires 75 M steps a second and none
//! of this changes anything — at which point [`sustainable`] answers `CLOCK` on its own and the
//! whole module retires without being deleted. It is filed as a bypass in
//! `research/04-bypass-ledger.md` with exactly that retirement condition.
//!
//! **Nothing here reads a clock, a file or an environment.** It is arithmetic over numbers the
//! caller measured, which is what lets the recipes go on pinning `--clock=` and be unaffected.

use crate::CLOCK;

/// The lowest clock this program will choose on its own.
///
/// Not a preference — a guard against one bad measurement. 5 is the accelerant this project ran at
/// for months, so it is known to boot every firmware here; below it the machine's own sense of time
/// runs so far ahead of the work that a game's frame delays expire before they are asked for. A
/// host that genuinely cannot sustain 5 M steps a second is a host this clamp will lie to, and it
/// will lie in the direction of *the iPod is too fast*, which is visible, rather than *the iPod has
/// stopped*, which is not.
pub const FLOOR: usize = 5;

/// The highest. **The real part, and never above it**: an iPod that ran faster than 75 MHz would be
/// as unfaithful as one running in slow motion, and a host with headroom should spend it on
/// [`CLOCK`] rather than on a number no PP5021C has.
pub const CEILING: usize = CLOCK;

/// The clock at which `steps` retired in `wall_secs` would have been real time.
///
/// `steps` is **executed instructions plus halted cycles** —
/// [`Machine::steps`](crate::Machine::steps) — because both advance the simulated clock at the same
/// rate. Counting only `executed` would name a clock that is too low by whatever fraction of the
/// window the core spent asleep, which on a booted iPod is nearly all of it.
///
/// `None` for an unmeasured window, and that is the distinction this repository keeps being burned
/// by: no wall time and no steps is *nobody has looked yet*, which must not render as a speed of
/// zero and must not choose a clock.
pub fn sustainable(steps: u64, wall_secs: f64) -> Option<usize> {
    // `is_finite` and then the comparison, rather than a negated `>`: a NaN window is unmeasured
    // like a zero one, and `!(x > 0.0)` says so only by accident of how NaN compares — which is
    // exactly the reading clippy's `neg_cmp_op_on_partial_ord` is about.
    if steps == 0 || !wall_secs.is_finite() || wall_secs <= 0.0 {
        return None;
    }
    let per_usec = steps as f64 / wall_secs / 1_000_000.0;
    Some((per_usec.round() as usize).clamp(FLOOR, CEILING))
}

/// How fast the iPod is going, as a fraction of the real part, given the clock in force and the one
/// the host can sustain.
///
/// This is the ratio §12.8 of `docs/GUI.md` could not state a divisor for and therefore did not
/// draw. It has one now, and it is not the instruction rate against 75 M: it is *simulated seconds
/// per wall second*, which is what a person watching the screen is actually measuring.
pub fn real_time_fraction(clock_in_force: usize, sustained: usize) -> f64 {
    if clock_in_force == 0 {
        return 0.0;
    }
    sustained as f64 / clock_in_force as f64
}

/// **The wheel's click spacing, at a given clock**, in instructions' worth of simulated time.
///
/// A rotation reaches the firmware one click at a time, and the interval that matters is the one
/// the firmware's wheel poll *sees* — which is in the machine's clock, not the host's. 4 ms is a
/// thumb; anything much faster is an input flood whose measured consequence (31 frames posted, 18
/// dropped unread at RetailOS's language menu) `parse_wheel_script` writes up.
///
/// **It is a duration and not a constant, and this is the second place that had to learn it.** A
/// hard 20 000 was 4 ms at `--clock=5` and became 266 µs when the default moved to 75; `trace`'s
/// default was fixed on 2026-09-07 after three weeks of being fifteen times wrong, and the window's
/// was a literal `300_000` that happened to be right at exactly one clock. Both call this now, so
/// there is no third place for the pair to disagree in.
pub fn wheel_click_gap(clock: usize) -> u64 {
    4_000 * clock.max(1) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The arithmetic, at the two speeds the issue names: a host doing 14.8 M steps a second is a
    /// 15 MHz iPod, and one doing the part's own rate is the part.
    #[test]
    fn the_sustainable_clock_is_steps_per_wall_microsecond() {
        assert_eq!(sustainable(14_800_000, 1.0), Some(15));
        assert_eq!(sustainable(75_000_000, 1.0), Some(75));
        assert_eq!(sustainable(148_000_000, 10.0), Some(15));
    }

    /// **The fixed point, which is what makes "measure once and hold it" honest.** A machine that
    /// has already been moved to the clock it can sustain measures the same clock again — so a
    /// recalibration is not a thing that drifts every time it is asked for.
    #[test]
    fn measuring_a_machine_already_at_its_sustainable_clock_names_the_same_clock() {
        // 15 M steps a second is the host. At clock 75 that is a fifth of real time…
        let first = sustainable(15_000_000, 1.0).unwrap();
        assert_eq!(first, 15);
        assert!((real_time_fraction(75, first) - 0.2).abs() < 0.001);
        // …and having moved to 15, the same host measures 15 again and is at real time.
        let again = sustainable(15_000_000, 1.0).unwrap();
        assert_eq!(again, first);
        assert!((real_time_fraction(first, again) - 1.0).abs() < 0.001);
    }

    /// Both ends, and the reason each exists. The ceiling is the part; the floor is a guard.
    #[test]
    fn the_clock_is_held_between_the_floor_and_the_real_part() {
        assert_eq!(sustainable(4_000_000_000, 1.0), Some(CEILING), "no iPod is faster than 75 MHz");
        assert_eq!(CEILING, CLOCK);
        assert_eq!(sustainable(1_000, 1.0), Some(FLOOR), "a stalled host is clamped, not obeyed");
    }

    /// **An unmeasured window is not a speed of zero**, and neither half of the pair may be
    /// invented from the other.
    #[test]
    fn nothing_measured_chooses_no_clock() {
        assert_eq!(sustainable(0, 1.0), None, "no steps");
        assert_eq!(sustainable(1_000_000, 0.0), None, "no wall time");
        assert_eq!(sustainable(0, 0.0), None);
        // A window whose wall time is not a number is unmeasured, not a clock. `f64::NAN` compares
        // false against everything, so a bare `<=` would have let it through into the division and
        // out the other side as `FLOOR` — a clamp turning nonsense into a plausible answer.
        assert_eq!(sustainable(1_000_000, f64::NAN), None, "an unmeasurable window");
        assert_eq!(sustainable(1_000_000, -1.0), None, "and time does not run backwards");
    }

    /// 4 ms at every clock — **the pair, because a constant satisfies either one alone.** This is
    /// `trace`'s own regression test, held here because this is now where the number lives.
    #[test]
    fn the_click_gap_is_four_milliseconds_at_every_clock() {
        for clock in [1usize, 5, 15, CLOCK, 75] {
            assert_eq!(
                wheel_click_gap(clock) / clock as u64,
                4_000,
                "{} instructions at {clock}/us is not 4 ms",
                wheel_click_gap(clock)
            );
        }
        // The two the corpus names by hand: research/04's ledger row for the clock change, and the
        // literal `ipod-gui` used to carry.
        assert_eq!(wheel_click_gap(5), 20_000);
        assert_eq!(wheel_click_gap(75), 300_000);
    }
}
