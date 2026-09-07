//! **The macOS speaker: `NSSound` over a synthesised WAV.** `docs/GUI.md` §21.8.
//!
//! One implementation of `trackpad::Detents`, and it is the *faithful* one — a real 5G clicks
//! through a piezo and makes a sound, so this is the part being modelled and the actuator beside it
//! is the enhancement.
//!
//! **`NSSound` and not a canned system sound**, and that is the whole point of the file above.
//! `NSSound::soundNamed:` would hand back Tink or Pop — somebody else's click, at somebody else's
//! pitch, saying nothing about what the guest wrote to `PWM0_CTRL`. `initWithData:` takes a
//! container we built out of the register's own low byte and the length its enable bit was actually
//! set for, so the sound is a *rendering of a measurement* and is wrong in a way a person can hear.
//!
//! **No new crates.** `objc2-app-kit`'s `NSSound` and `objc2-foundation`'s `NSData` are feature
//! names inside two crates that are compiled either way, exactly like the nine `§21.8` already
//! turned on for `NSTouch` and the actuator.

use std::cell::RefCell;
use std::collections::HashMap;

use objc2::rc::{Allocated, Retained};
use objc2::AllocAnyThread;
use objc2_app_kit::NSSound;
use objc2_foundation::NSData;

use super::{sound, Tone};
use crate::trackpad::{Detents, Mark};

/// The sounds already built, keyed by the pitch's bits and the length in microseconds.
///
/// **A cache and not an optimisation of one allocation.** `NSSound::initWithData:` parses a
/// container and hands back an object holding a decoded buffer; doing that per click would put a
/// parse on the path of a 60 Hz tick. The key is both halves of the sound, because two clicks of
/// the same pitch and different lengths are two sounds — and the vocabulary is *tiny*: research/05
/// found four wave constants in the whole of Apple's image and one in Rockbox's, so this map
/// reaches a handful of entries and stops.
///
/// It is unbounded for that reason, and the reason is worth stating rather than assuming: if a
/// firmware nobody has run turns out to sweep the register, this grows with the sweep. That would
/// be a discovery about the firmware, and it would show up in `Piezo::waves` — which is uncapped
/// for the same reason — long before it showed up as memory.
/// `(pitch bits, microseconds) -> the sound, or the `None` macOS answered with.`
type Built = HashMap<(u32, u32), Option<Retained<NSSound>>>;

struct Speaker {
    built: RefCell<Built>,
}

impl Speaker {
    /// Build or fetch the sound for one tone. `None` when macOS would not take the bytes, which is
    /// cached too — a container it rejected once it will reject every time, and re-parsing it sixty
    /// times a second would be the failure costing more than the feature.
    fn sound_for(&self, hz: f32, usec: u32) -> Option<Retained<NSSound>> {
        let key = (hz.to_bits(), usec);
        if let Some(s) = self.built.borrow().get(&key) {
            return s.clone();
        }
        let bytes = super::wav(hz, usec);
        let data = NSData::with_bytes(&bytes);
        let alloc: Allocated<NSSound> = NSSound::alloc();
        let s = NSSound::initWithData(alloc, &data);
        self.built.borrow_mut().insert(key, s.clone());
        s
    }
}

impl Detents for Speaker {
    fn describe(&self) -> &'static str {
        "the piezo (a square wave at the period the guest wrote)"
    }

    /// **A claim about the build, exactly as the actuator's is**, and for a related reason: this can
    /// say that macOS accepted the container, and it cannot say that anything came out of a speaker.
    /// The volume may be down, the output may be a device nobody is listening to, and `NSSound::play`
    /// answers `true` for all of it.
    ///
    /// What it *can* do, which the actuator cannot, is fail honestly at the first click — see
    /// [`Speaker::sound_for`]'s `None`, which reaches the log through `click`.
    fn present(&self) -> bool {
        true
    }

    /// **The click the guest made.** Rate-limited by `Feedback` on the same schedule as the
    /// actuator, so the two never come out of step with each other.
    fn click(&self, tone: Option<Tone>) {
        let (hz, usec) = sound(tone);
        let Some(s) = self.sound_for(hz, usec) else {
            crate::trackpad::note("the click could not be built as a sound — nothing will be heard");
            return;
        };
        // **Rewound rather than left to finish.** `NSSound::play` on an object that is already
        // playing answers `false` and does nothing, so two clicks inside one tone's length would be
        // one click. A tone is milliseconds and the rate limit is 6 ms, so this is nearly never
        // taken — but "nearly never" is the case that goes unnoticed.
        if s.isPlaying() {
            s.stop();
        }
        s.play();
    }

    /// **Silent, and that is the design rather than an omission.**
    ///
    /// A `Mark` is a line drawn on a *rectangle of glass* — where the trackpad's modelled wheel
    /// meets its modelled centre button. It is the window putting back an edge the real part had in
    /// its moulding and this surface does not have, and it is felt for exactly that reason.
    ///
    /// A **sound** for it would be a different thing entirely: a noise coming out of an iPod that
    /// the iPod never made, in the same channel the faithful click uses, on a machine that might not
    /// even be running. The actuator can say *you crossed a boundary on your trackpad* without
    /// claiming anything about the device; a speaker cannot.
    fn mark(&self, _: Mark) {}
}

/// The sink, if this build has one. Always `Some` on macOS — see [`Speaker::present`] for what that
/// does and does not claim.
pub fn speaker() -> Option<Box<dyn Detents>> {
    Some(Box::new(Speaker { built: RefCell::new(HashMap::new()) }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **macOS accepts the container**, which is the one half of this path software can check —
    /// and it is the half that would fail silently. A WAV with a wrong `data` size, a bad block
    /// align or a header field in the wrong order is not a compile error and not a panic: it is
    /// `initWithData:` answering `nil`, and a program that ignored that would be a click nobody
    /// hears and no line anywhere saying why.
    ///
    /// It stops at `duration`, deliberately. `play()` would make a noise on the operator's machine
    /// during `cargo test`.
    ///
    /// **How to make it go red:** write the `data` chunk's size as the sample *count* rather than
    /// the byte count in `click::riff`, which is the classic error and halves every click.
    #[test]
    fn macos_takes_the_container_and_agrees_about_how_long_it_is() {
        let s = Speaker { built: RefCell::new(HashMap::new()) };
        for (hz, usec, secs) in [(1073.0f32, 3_000u32, 0.003f64), (1002.0, 5_200, 0.0052)] {
            let sound = s.sound_for(hz, usec).expect(
                "macOS refused the synthesised WAV — the container is malformed, and nothing about \
                 that would show up as a compile error or a panic",
            );
            let got = sound.duration();
            assert!(
                (got - secs).abs() < 0.001,
                "macOS reads the {usec} us click as {got} s, so the header and the samples disagree"
            );
        }
    }

    /// The cache is keyed on the whole tone, so two pitches are two sounds and the same tone twice
    /// is one — which is what keeps a container parse off a 60 Hz tick.
    #[test]
    fn one_sound_is_built_per_tone_and_not_per_click() {
        let s = Speaker { built: RefCell::new(HashMap::new()) };
        s.sound_for(1073.0, 3_000);
        s.sound_for(1073.0, 3_000);
        assert_eq!(s.built.borrow().len(), 1, "the same tone was built twice");
        s.sound_for(1002.0, 3_000);
        s.sound_for(1073.0, 5_200);
        assert_eq!(s.built.borrow().len(), 3, "a different pitch or length is a different sound");
    }
}
