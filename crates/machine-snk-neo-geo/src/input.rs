//! The AES controller.

use serde::{Deserialize, Serialize};

/// One controller's buttons, `true` while held.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Buttons {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub a: bool,
    pub b: bool,
    pub c: bool,
    pub d: bool,
    pub start: bool,
    pub select: bool,
}

impl Buttons {
    /// The joystick and four buttons as the `$300000`/`$340000` byte reads
    /// them: up, down, left, right, A, B, C, D from bit 0, active low.
    #[must_use]
    pub fn port(&self) -> u8 {
        let held = [
            self.up, self.down, self.left, self.right, self.a, self.b, self.c, self.d,
        ];
        !held
            .iter()
            .enumerate()
            .fold(0, |acc, (bit, &on)| acc | (u8::from(on) << bit))
    }

    /// Start in bit 0 and select in bit 1, active low, as the `$380000`
    /// word carries them.
    #[must_use]
    pub fn start_select(&self) -> u8 {
        !(u8::from(self.start) | u8::from(self.select) << 1) & 3
    }
}
