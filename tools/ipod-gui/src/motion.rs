//! Whether this machine's owner has asked for less movement.
//!
//! `docs/GUI.md` §8.4. **Slint has no reduced-motion signal of its own** — a grep for
//! `reduced.motion` across `i-slint-core`, `i-slint-common` and `builtins.slint` returns nothing —
//! so it is read per platform here and pushed into the `Motion` global as a scale of 1 or 0.
//!
//! **The multiplication lives in the `Motion` global, not at the use sites.** A use site writes
//! `Motion.gentle` and cannot get it wrong; a use site writing `Metric.tight * Motion.scale` can
//! forget the second half, and a forgotten one is a silent hole no test would see.
//!
//! **A scale of zero lands on the animation's final value rather than freezing it.** Verified
//! rather than assumed, because the whole of reduced motion rests on it:
//! `i-slint-core-1.17.1/properties_animations.rs:123-125` short-circuits `duration <= 0` to
//! `AnimationState::Done`, and `Done` returns `to_value` (`:156-161`). The structural change still
//! happens; only its animation does not.
//!
//! No toolkit in this file: it asks the platform, not the window.

/// `1.0` normally, `0.0` where the platform says the person has asked for reduced motion.
///
/// Never fails and never blocks. On a platform with no such setting the answer is `1.0`, which is
/// the honest one — it is what the machine is telling us.
pub fn scale() -> f32 {
    if reduced() {
        0.0
    } else {
        1.0
    }
}

/// macOS: `NSWorkspace.accessibilityDisplayShouldReduceMotion`.
///
/// **Main thread only** — it reaches AppKit, exactly as `client_height` does. Called once, before
/// the event loop runs.
#[cfg(target_os = "macos")]
fn reduced() -> bool {
    objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
}

/// Windows: `SPI_GETCLIENTAREAANIMATION`, which is **on** when animation is wanted — so the
/// reduced-motion answer is its negation, and reading it the other way round would turn animation
/// off for everybody who never touched the setting.
#[cfg(target_os = "windows")]
fn reduced() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION,
    };

    let mut animations: i32 = 1;
    // SAFETY: `animations` is a live, correctly sized BOOL and SPI_GETCLIENTAREAANIMATION writes
    // exactly one.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            (&mut animations as *mut i32).cast(),
            0,
        )
    };
    // A call that failed is not an observation of a preference. Same rule as `Presence::exists`:
    // an error is not a fact about the machine.
    ok != 0 && animations == 0
}

/// Everywhere else there is no such setting to read, and saying so is better than guessing.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn reduced() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two values are the only two, and `0.0` is the one the animation code is proved against.
    ///
    /// It cannot assert *which* one this machine gives — that is the operator's own accessibility
    /// setting and a test that pinned it would fail on a machine where somebody had turned it on.
    #[test]
    fn the_scale_is_one_or_zero_and_nothing_between() {
        let s = scale();
        assert!(
            s == 0.0 || s == 1.0,
            "the motion scale is {s}; a fraction would make every duration a different animation \
             rather than the same one turned off"
        );
        assert_eq!(scale(), if reduced() { 0.0 } else { 1.0 });
    }
}
