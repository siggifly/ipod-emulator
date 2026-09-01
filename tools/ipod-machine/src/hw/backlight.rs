//! The panel's dimmer — a pulse-width protocol on one GPIO pin.

use crate::{Capped, BACKLIGHT_PIN, BACKLIGHT_STEP_USEC};

/// The panel's dimmer, as the pulses on [`GPIOB_OUTPUT_VAL`] leave it.
#[derive(Clone, Debug)]
pub struct Backlight {
    /// 1..32. Starts at 16, which is where Rockbox's driver says the circuit wakes up, and is the
    /// only value in this model that is assumed rather than derived.
    pub level: u8,
    /// `usec` at which the pin went low, while it is low.
    low_since: Option<u32>,
    pub steps_up: u64,
    pub steps_down: u64,
    /// Every pulse's low width, in microseconds, in order.
    ///
    /// **[`BACKLIGHT_STEP_USEC`] is inferred, not measured.** It comes from Rockbox's driver, whose
    /// two delays are 10 µs and 200 µs — and Rockbox is not the firmware this emulator runs. If
    /// Apple's own delays fall on the same side of the threshold, every pulse steps the same way,
    /// the level walks to a rail, and the dimmer looks like it does nothing. That failure is
    /// invisible from the level alone, which is why the widths are kept rather than just the
    /// verdict they produced.
    pub widths: Capped<u32>,
}

impl Default for Backlight {
    fn default() -> Self {
        Self {
            level: 16,
            low_since: None,
            steps_up: 0,
            steps_down: 0,
            widths: Capped::new(256),
        }
    }
}

impl Backlight {
    /// One write of the port. Returns true if the level moved.
    pub fn port_written(&mut self, val: u32, usec: u32) -> bool {
        let high = val & BACKLIGHT_PIN != 0;
        match (self.low_since, high) {
            // Falling edge: start timing.
            (None, false) => {
                self.low_since = Some(usec);
                false
            }
            // Rising edge: the width of the low decides the direction.
            (Some(at), true) => {
                self.low_since = None;
                let width = usec.wrapping_sub(at);
                self.widths.push(width);
                if width < BACKLIGHT_STEP_USEC {
                    self.steps_up += 1;
                    self.level = (self.level + 1).min(32);
                } else {
                    self.steps_down += 1;
                    self.level = self.level.saturating_sub(1).max(1);
                }
                true
            }
            _ => false,
        }
    }
}
