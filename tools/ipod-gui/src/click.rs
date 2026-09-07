//! **The click, as a sound.** `docs/GUI.md` §21.8.
//!
//! A real 5G clicks through a **piezo** — the linear actuator a fingertip feels is what modders fit
//! in its place. So the audible click is the *faithful* half of this and the haptic one is the
//! enhancement, and this is the file that can make a machine with no actuator — every Mac that is
//! not a laptop, every pre-2015 pad, every other platform — say the same thing to a person.
//!
//! **It waits on nothing.** `docs/GUI.md` §15 defers audio to 1.0, and that is about the Wolfson
//! codec: music, the DAC, the I²S stream. The piezo is not the codec and never was — it is
//! `PWM0_CTRL` at `0x7000A000`, one register on the PortalPlayer, and the two are as related as a
//! car's horn and its stereo.
//!
//! # What the guest actually says, and what is inferred from it
//!
//! `ipod-machine`'s `hw/piezo.rs` is a recorder and this is the renderer, so the seam between them
//! is worth stating in both directions:
//!
//! | | where it comes from |
//! |---|---|
//! | **that a click happened** | measured — `Piezo::fires`, bit 31 going clear to set |
//! | **how long it ran** | measured — the simulated microseconds the enable bit stayed set |
//! | **what pitch it was** | **inferred**, and this is the one soft step. See [`hz`] |
//! | what it sounded like | a square wave, because a PWM channel driving a piezo is a square wave |
//!
//! **The pitch is the honest weak point and it is stated rather than hidden.** research/05's own
//! words: Apple's four wave constants run through *"Rockbox's own independently-derived frequency
//! relation — `piezo.c` returns `91225/hz`"* land on 1073, 507, 633 and 815 Hz, and *"four constants
//! from Apple's image landing in the audible beeper band under a formula from a different codebase
//! is a coincidence worth recording; it is not proof, and no run has yet confirmed a frequency."*
//! Nothing here can confirm one either. What this file does is make the inference **audible**, which
//! is the cheapest way it will ever be falsified: a person who has heard a real 5G and hears this
//! one knows within a second whether the relation holds.

/// **A tone the guest ran**: the wave word with the enable bit masked off, and how long bit 31
/// stayed set, in simulated microseconds.
///
/// Both halves are measured by `ipod_machine::Piezo`. This is a copy of that type rather than a
/// re-export because it crosses into the window's feedback contract, where nothing else names a
/// machine type — `trackpad::Detents` is implemented by an AppKit sink and by two counters in a
/// test, and none of them should have to know what an emulator is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Tone {
    pub wave: u32,
    pub usec: u32,
}

/// **Apple's own tick, for a click whose tone has not been observed to finish.** 3 ms.
///
/// `0x001B91FC` — the call site behind RetailOS's settings bit, and the only one the measured run
/// ever reached — passes `0xBB8` microseconds as the duration. So this is Apple's number rather than
/// a taste, and it is used for exactly one case: the click that has just been counted while its own
/// stop has not yet been seen. It is **not** a substitute for a measurement, which is why
/// `Piezo::tone` is an `Option` and not a zero.
pub const APPLE_TICK_USEC: u32 = 3_000;

/// The wave Apple's measured run played, every time — `0x0080_0055`, which is bit 23 and a period
/// of 0x55. Used for the same one case [`APPLE_TICK_USEC`] is.
pub const APPLE_WAVE: u32 = 0x0080_0055;

/// **Rockbox's frequency relation, inverted.** `piezo_set_period` returns `91225 / hz`, so a period
/// of `p` is `91225 / p` hertz.
///
/// The period is the **low byte** of the wave. That is read off the four constants research/05
/// found in Apple's image — `0x55`, `0xB4`, `0x90`, `0x70` — and off Rockbox's own
/// `form_and_period` of `0x0080005B`, whose top half is the form bit both firmwares set and whose
/// low byte is `0x5B`. Two firmwares sharing no authorship, both with the period in the bottom
/// eight bits.
///
/// `None` for a period of zero, which is a divide and not a tone.
///
/// **Unconfirmed by measurement, and deliberately so rather than quietly.** See the module note:
/// this is the one inferred step in the whole path, and no run in this repository has yet observed
/// a frequency to check it against.
pub fn hz(wave: u32) -> Option<f32> {
    let period = wave & 0xff;
    (period != 0).then(|| 91_225.0 / period as f32)
}

/// Samples a second. CD rate — every platform's audio stack takes it without resampling, and a
/// 1 kHz square wave has 44 samples a cycle at it, which is more than enough edge for a tick.
pub const RATE: u32 = 44_100;

/// **How short and how long a click may be**, as a sanity bound on a number that comes out of an
/// emulated register rather than as a policy about the part.
///
/// The floor is one millisecond: below that there is not a whole cycle of the lowest tone in
/// Apple's vocabulary (507 Hz is 2 ms a cycle) and what comes out is a pop rather than a pitch. The
/// ceiling is a second, which is over twice the longest duration research/05 found at any call site
/// (400 ms, in the alarm's melody) — so it can only ever be reached by a register doing something
/// nobody has seen, and it bounds a `Vec` rather than trimming a click anybody made.
pub const MIN_USEC: u32 = 1_000;
/// See [`MIN_USEC`].
pub const MAX_USEC: u32 = 1_000_000;

/// **The edge ramp, and it is the host's artefact rather than the part's.** 0.3 ms.
///
/// A piezo disc has mass, so it does not start or stop instantly; a buffer of samples handed to a
/// DAC does, and a hard edge at a non-zero sample is a step the speaker reproduces as a click of
/// its own — *louder*, on most hardware, than the 3 ms tone it is bracketing. So the first and last
/// 0.3 ms are ramped, which is under a tenth of the shortest tone Apple sends and inaudible as an
/// envelope.
///
/// **This is a departure from the register and is stated as one.** It is not modelling the disc's
/// mechanical response — nothing here has measured that — it is stopping the *playback* from adding
/// a sound the part did not make.
const RAMP_USEC: u32 = 300;

/// **Peak amplitude, and it is a choice rather than a measurement.** 0.22 of full scale.
///
/// The register says nothing about loudness: it is one enable bit and a period, and the piezo's
/// actual output depends on the disc, the case it is glued to and the voltage — none of which is
/// modelled or knowable from here. So this is set by ear against the thing it has to be: audible
/// over a room, and quieter than anything else the host is playing, because a click that talks over
/// music is worse than no click.
const PEAK: f32 = 0.22;

/// **One click, as 16-bit mono PCM in a RIFF container.**
///
/// A square wave, because that is what a PWM channel driving a piezo is — the register's own name
/// is `PWM0_CTRL` and the low byte is a period. Not a sine: a sine is what a *tone generator*
/// produces, and rendering one here would be a smoother sound than the part makes.
///
/// WAV rather than raw samples because the one platform sink that exists takes a container
/// (`NSSound::initWithData:`), and every other platform's does too. The header is 44 bytes and is
/// written by hand rather than by a crate — this is the whole of what is needed from an audio
/// format, and a dependency for it would be a graph entry to justify at every audit.
pub fn wav(hz: f32, usec: u32) -> Vec<u8> {
    let usec = usec.clamp(MIN_USEC, MAX_USEC);
    let n = ((u64::from(usec) * u64::from(RATE)) / 1_000_000) as usize;
    let ramp = (((u64::from(RAMP_USEC) * u64::from(RATE)) / 1_000_000) as usize).max(1).min(n / 2);
    let mut pcm: Vec<u8> = Vec::with_capacity(n * 2);
    for i in 0..n {
        // The square: half a period high, half low. `fract` on the phase rather than a counter, so
        // a frequency that is not a whole number of samples per cycle does not drift.
        let phase = (i as f32 / RATE as f32) * hz;
        let level = if phase.fract() < 0.5 { PEAK } else { -PEAK };
        // Linear in and out. `ramp` is at most half the buffer, so the two never overlap.
        let gain = if i < ramp {
            i as f32 / ramp as f32
        } else if i + ramp >= n {
            // `n - 1 - i` and not `n - i`: the last sample must be **zero**, and off by one it is
            // `PEAK / ramp` — a step of 554 out of 7208, which is the artefact the ramp exists to
            // remove, one thirteenth as loud and still a click of the host's own.
            (n - 1 - i) as f32 / ramp as f32
        } else {
            1.0
        };
        pcm.extend_from_slice(&((level * gain * i16::MAX as f32) as i16).to_le_bytes());
    }
    riff(&pcm)
}

/// The 44-byte canonical WAV header, and the samples after it. Mono, 16-bit, [`RATE`].
fn riff(pcm: &[u8]) -> Vec<u8> {
    let data = pcm.len() as u32;
    let mut out = Vec::with_capacity(44 + pcm.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // PCM chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // format: PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // channels: mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes()); // bytes a second
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits a sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    out.extend_from_slice(pcm);
    out
}

/// **What the sound for one click is**, resolved from what the guest was observed to do.
///
/// `None` is the case the whole `Option` exists for: a click has been counted and its own stop has
/// not been seen yet, so there is a measurement coming and it is not here. Apple's own constants
/// stand in for exactly that gap — see [`APPLE_TICK_USEC`] — and the *next* click will carry the
/// measurement, because a tone is milliseconds and the reader is a 16 ms tick.
pub fn sound(tone: Option<Tone>) -> (f32, u32) {
    let t = tone.unwrap_or(Tone { wave: APPLE_WAVE, usec: APPLE_TICK_USEC });
    (hz(t.wave).unwrap_or(1_073.0), t.usec)
}

/// **How many clicks the guest has made since anyone last looked.**
///
/// `Piezo::fires` is a census that only grows, so the window's 60 Hz tick reads a *difference* — and
/// this is the piece that owns the difference, in a file with no toolkit and no window in it, so it
/// can be tested against every sequence a machine can produce rather than only the ones a running
/// one happens to.
///
/// **A count that goes backwards is a different machine, not four billion clicks.** Power off and
/// on, a boot into diagnostics, a device swapped on the bench: every one of them builds a fresh
/// `Machine` whose `Piezo` starts at zero. A subtraction there underflows into a number that would
/// ask the actuator for every pulse it can produce for the rest of the session, which is the single
/// worst thing this path can do to somebody. So a fall is a **re-anchor** and never a click.
#[derive(Default)]
pub struct Watch {
    seen: u64,
}

impl Watch {
    /// The clicks since the last call, differenced from zero on the first.
    ///
    /// **Zero is the right anchor and not merely the convenient one.** A `Watch` is made when a
    /// machine is put on the bench, and every machine's `Piezo` starts at zero — including a
    /// restored one, because the snapshot format serialises regions and not peripherals. So the
    /// first call has nothing to catch up on, and if that ever stops being true it is the same case
    /// as a power cycle and the fall rule below already covers it.
    pub fn since(&mut self, fires: u64) -> u32 {
        let n = fires.saturating_sub(self.seen);
        self.seen = fires;
        // Saturating rather than `as`: a `u64` census cannot exceed a `u32` of clicks in any run a
        // person sits through, and if it somehow did, the actuator's own rate limit is what the
        // surplus would meet — not an integer wrapping to something small.
        n.min(u64::from(u32::MAX)) as u32
    }
}

#[cfg(target_os = "macos")]
mod mac;

/// **The speaker, where the platform has one.** `None` on a build with no sink, which is what
/// `Feedback::describe` then says out loud.
#[cfg(target_os = "macos")]
pub use mac::speaker;

/// See the macOS arm. Every other platform has no audio route in this program yet, and says so
/// rather than pretending: `Feedback::describe` reports *"nothing"* and `docs/GUI.md` §21.8 is where
/// that is written down.
#[cfg(not(target_os = "macos"))]
pub fn speaker() -> Option<Box<dyn crate::trackpad::Detents>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The four waves research/05 read out of Apple's image, and Rockbox's one**, through the
    /// relation that was derived on the other side. If this file's byte-picking is wrong the
    /// numbers land nowhere near a beeper.
    ///
    /// Red by masking `0xffff` instead of `0xff`: `0x0080_0055` becomes period 0x0055 still — the
    /// same — but `0x0080_00B4` and the rest are unchanged too, so use bit 23 to see it: mask the
    /// whole word and `0x0080_0055` is period 8_388_693 and the answer is 0.01 Hz.
    #[test]
    fn the_waves_in_apples_image_are_the_frequencies_research_05_records() {
        for (wave, want) in [
            (0x0080_0055u32, 1073.0f32),
            (0x0080_00b4, 507.0),
            (0x0080_0090, 633.0),
            (0x0080_0070, 815.0),
            // Rockbox's own, from the other firmware entirely.
            (0x0080_005b, 1002.0),
        ] {
            let got = hz(wave).expect("a period");
            assert!(
                (got - want).abs() < 1.0,
                "wave {wave:#010x} is {got} Hz, and research/05 records {want}"
            );
        }
        // A period of zero is a divide, and it is refused rather than answered with an infinity.
        assert_eq!(hz(0x0080_0000), None);
        assert_eq!(hz(0), None);
    }

    /// **The container is one macOS will accept**, checked field by field rather than by whether it
    /// happened to play — a WAV with a wrong `data` size is silently truncated by some decoders and
    /// rejected by others, and neither says which.
    #[test]
    fn the_click_is_a_whole_riff_wave_of_the_length_that_was_asked_for() {
        let w = wav(1073.0, 3_000);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(&w[8..12], b"WAVE");
        assert_eq!(&w[12..16], b"fmt ");
        assert_eq!(&w[36..40], b"data");
        let samples = (RATE as u64 * 3_000 / 1_000_000) as usize; // 132
        assert_eq!(samples, 132);
        assert_eq!(u32::from_le_bytes(w[40..44].try_into().unwrap()), samples as u32 * 2);
        assert_eq!(w.len(), 44 + samples * 2);
        // The two sizes in the header agree with the file, which is the pair a hand-written header
        // gets wrong.
        assert_eq!(u32::from_le_bytes(w[4..8].try_into().unwrap()) as usize + 8, w.len());
        assert_eq!(u32::from_le_bytes(w[24..28].try_into().unwrap()), RATE);
    }

    /// **The wave is square and it starts and ends at silence.**
    ///
    /// Both halves matter and they are different claims: the square is the part being modelled (a
    /// PWM channel is not a sine), and the silent ends are the *host's* artefact being kept out —
    /// a buffer that begins at full amplitude is a step the speaker reproduces louder than the tone.
    ///
    /// Red by deleting the `gain` multiply: the first sample becomes 7208 and the assertion below
    /// it fails.
    #[test]
    fn the_wave_is_square_and_ramped_at_both_ends() {
        let w = wav(1_000.0, 10_000);
        let s: Vec<i16> = w[44..]
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();
        assert_eq!(s.len(), 441);
        assert_eq!(s[0], 0, "the first sample is not silence, so the click begins with a step");
        assert_eq!(*s.last().expect("samples"), 0, "and it ends with one");

        // In the middle the gain is 1, and at 1 kHz into 44 100 Hz a half-cycle is 22 samples. Every
        // sample away from the ramps is at full amplitude, one sign or the other — which is what
        // makes it square rather than anything smoother.
        let peak = (PEAK * i16::MAX as f32) as i16;
        let ramp = 13; // 300 us at 44.1 kHz
        for (i, v) in s.iter().enumerate().skip(ramp + 1).take(s.len() - 2 * ramp - 2) {
            assert_eq!(v.abs(), peak, "sample {i} is {v}, so the wave is not square");
        }
        // And it does change sign, or "square" would be satisfied by a constant.
        assert!(s.iter().any(|v| *v > 0) && s.iter().any(|v| *v < 0));
    }

    /// A duration out of an emulated register is bounded rather than trusted. Both ends, and the
    /// bound is a sanity limit on a `Vec` rather than a trim of anything a firmware sends: the
    /// longest duration research/05 found at any call site is 400 ms.
    #[test]
    fn an_absurd_duration_is_bounded_at_both_ends_rather_than_allocated() {
        assert_eq!(wav(1_000.0, 0).len(), 44 + 44 * 2, "a zero-length click is floored at 1 ms");
        assert_eq!(wav(1_000.0, u32::MAX).len(), 44 + 44_100 * 2, "and a huge one at 1 s");
        // The whole of Apple's own vocabulary is inside the bounds and therefore untouched.
        for usec in [3_000u32, 80_000, 150_000, 200_000, 400_000] {
            let n = (RATE as u64 * u64::from(usec) / 1_000_000) as usize;
            assert_eq!(wav(1_000.0, usec).len(), 44 + n * 2, "{usec} us was clamped");
        }
    }

    /// **A click whose tone has not been observed falls back to Apple's own constants**, and says
    /// which case it is in rather than rendering a zero-length click.
    #[test]
    fn a_click_with_no_measured_tone_uses_apples_own_numbers() {
        let (hz_none, usec_none) = sound(None);
        assert_eq!(usec_none, APPLE_TICK_USEC);
        assert!((hz_none - 1073.0).abs() < 1.0);
        // And a measured one is used in preference — both halves, because a renderer that took the
        // pitch and ignored the length would pass an assertion about the pitch alone.
        let (h, u) = sound(Some(Tone { wave: 0x0080_005b, usec: 5_200 }));
        assert!((h - 1002.0).abs() < 1.0);
        assert_eq!(u, 5_200);
    }

    /// **A census read as a difference, and a machine that restarts is not four billion clicks.**
    ///
    /// The fall case is the one that matters: every power cycle builds a fresh `Machine` whose
    /// `Piezo` starts at zero, and a subtraction there underflows into a number that would ask the
    /// actuator for every pulse it can make for the rest of the session.
    ///
    /// **How to make it go red:** `fires - self.seen` instead of `saturating_sub`. In release the
    /// fall reports 18 446 744 073 709 551 607; in debug it panics.
    #[test]
    fn clicks_are_a_difference_and_a_machine_that_restarts_re_anchors() {
        let mut w = Watch::default();
        assert_eq!(w.since(0), 0, "a machine that has not clicked asked for a pulse");
        assert_eq!(w.since(1), 1);
        assert_eq!(w.since(1), 0, "the same census twice is one click, not two");
        assert_eq!(w.since(9), 8, "eight clicks inside one tick are eight");
        // The power cycle.
        assert_eq!(w.since(0), 0, "a fresh machine's zero was read as a flood");
        assert_eq!(w.since(1), 1, "and the next click on the new machine is one click");
    }

    /// The anchor is zero, which is where every machine's `Piezo` starts — so a `Watch` made
    /// alongside a machine has nothing to catch up on, and one made beside a census that is already
    /// high reports the difference rather than swallowing it.
    #[test]
    fn a_fresh_watch_differences_against_zero() {
        assert_eq!(Watch::default().since(0), 0);
        assert_eq!(Watch::default().since(4_000), 4_000);
    }
}
