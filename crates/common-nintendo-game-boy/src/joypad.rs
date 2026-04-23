//! Joypad matrix.
//!
//! The Game Boy joypad register at `$FF00` (`P1`/`JOYP`) exposes a
//! 4-bit input nibble plus two select bits. The CPU writes bits 4
//! and 5 to choose which group of four buttons is multiplexed onto
//! the input nibble:
//!
//! | Bit | Direction | Meaning                                      |
//! |-----|-----------|----------------------------------------------|
//! | 5   | write     | `0` selects the action group (A/B/Select/Start) |
//! | 4   | write     | `0` selects the direction group (Right/Left/Up/Down) |
//! | 3   | read      | Down / Start  (active LOW — pressed = 0)     |
//! | 2   | read      | Up / Select   (active LOW)                   |
//! | 1   | read      | Left / B      (active LOW)                   |
//! | 0   | read      | Right / A     (active LOW)                   |
//!
//! When both group-select bits are `0`, the read nibble is the AND
//! of the two groups (a button pressed in either group reads as 0).
//! When both are `1`, the nibble is `1111` (no buttons selected).
//!
//! Pressing any button while the joypad interrupt source is enabled
//! latches the joypad interrupt (`IF` bit 4) on the falling edge of
//! the input nibble. The matrix exposes [`JoypadMatrix::edge`] so
//! the machine layer can detect the edge for IRQ generation.

use serde::{Deserialize, Serialize};

/// Eight DMG buttons. The two groups of four are the action buttons
/// (`A`, `B`, `Select`, `Start`) and the direction buttons
/// (`Right`, `Left`, `Up`, `Down`).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum JoypadButton {
    Right,
    Left,
    Up,
    Down,
    A,
    B,
    Select,
    Start,
}

impl JoypadButton {
    /// Maps a button to its bit position (0..=3) within its group's
    /// nibble.
    #[must_use]
    pub const fn nibble_bit(self) -> u8 {
        match self {
            Self::Right | Self::A => 0,
            Self::Left | Self::B => 1,
            Self::Up | Self::Select => 2,
            Self::Down | Self::Start => 3,
        }
    }

    /// Returns the group this button belongs to.
    #[must_use]
    pub const fn group(self) -> JoypadSelect {
        match self {
            Self::Right | Self::Left | Self::Up | Self::Down => JoypadSelect::Direction,
            Self::A | Self::B | Self::Select | Self::Start => JoypadSelect::Action,
        }
    }
}

/// Which group of four buttons is currently routed to the input
/// nibble at `$FF00`. When `Both` is selected the nibble is the AND
/// of both groups; when `Neither`, the nibble reads as `0xF`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum JoypadSelect {
    /// Bit 4 = 0, bit 5 = 1 — direction buttons routed.
    Direction,
    /// Bit 4 = 1, bit 5 = 0 — action buttons routed.
    Action,
    /// Both selected (both bits 0) — AND of the two groups.
    Both,
    /// Neither selected (both bits 1) — reads as `0xF`.
    Neither,
}

impl JoypadSelect {
    /// Decode the select state from the upper nibble of a write to
    /// `$FF00`.
    #[must_use]
    pub const fn from_p1_byte(byte: u8) -> Self {
        let action_low = (byte & 0x20) == 0;
        let direction_low = (byte & 0x10) == 0;
        match (action_low, direction_low) {
            (true, true) => Self::Both,
            (true, false) => Self::Action,
            (false, true) => Self::Direction,
            (false, false) => Self::Neither,
        }
    }
}

/// Pressed-state of the eight buttons. `pressed == true` means the
/// button is currently held down (which on the bus reads as bit
/// `0` — active LOW).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct JoypadMatrix {
    direction: u8,
    action: u8,
    select: u8,
}

impl JoypadMatrix {
    /// Creates an all-released matrix with no select group active
    /// (P1 high nibble = `0x30` after reset).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            direction: 0,
            action: 0,
            select: 0x30,
        }
    }

    /// Sets the pressed state of one button.
    pub fn set(&mut self, button: JoypadButton, pressed: bool) {
        let bit = 1u8 << button.nibble_bit();
        let mask = match button.group() {
            JoypadSelect::Direction => &mut self.direction,
            JoypadSelect::Action => &mut self.action,
            // The button-to-group mapping never returns Both/Neither.
            _ => unreachable!(),
        };
        if pressed {
            *mask |= bit;
        } else {
            *mask &= !bit;
        }
    }

    /// Returns `true` if the button is currently pressed.
    #[must_use]
    pub const fn is_pressed(&self, button: JoypadButton) -> bool {
        let bit = 1u8 << button.nibble_bit();
        let mask = match button.group() {
            JoypadSelect::Direction => self.direction,
            JoypadSelect::Action => self.action,
            _ => 0,
        };
        (mask & bit) != 0
    }

    /// Updates the select state from a write to `$FF00`. Only bits
    /// 4-5 are observed; the low nibble is read-only.
    pub fn write_p1(&mut self, byte: u8) {
        // Preserve the unselected state (bits 6-7) and update the
        // two select bits. Bits 6-7 are wired high on real hardware
        // but the CPU sees them as `1`; the byte is otherwise stored
        // verbatim so reads round-trip.
        self.select = (byte & 0x30) | 0xC0;
    }

    /// Reads `$FF00`. The high nibble carries the select bits (with
    /// 6-7 wired high), the low nibble carries the input state.
    #[must_use]
    pub fn read_p1(&self) -> u8 {
        let pressed_nibble = match JoypadSelect::from_p1_byte(self.select) {
            JoypadSelect::Action => self.action,
            JoypadSelect::Direction => self.direction,
            JoypadSelect::Both => self.action | self.direction,
            JoypadSelect::Neither => 0,
        };
        // Pressed buttons read as 0 (active LOW); the unselected /
        // not-pressed bits read as 1.
        let input_nibble = !pressed_nibble & 0x0F;
        self.select | input_nibble
    }

    /// Returns `true` if any button currently selected by the
    /// `$FF00` write is pressed. The machine's joypad-IRQ source
    /// fires on the falling edge of any selected input bit.
    #[must_use]
    pub fn any_selected_pressed(&self) -> bool {
        let pressed_nibble = match JoypadSelect::from_p1_byte(self.select) {
            JoypadSelect::Action => self.action,
            JoypadSelect::Direction => self.direction,
            JoypadSelect::Both => self.action | self.direction,
            JoypadSelect::Neither => 0,
        };
        pressed_nibble != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_p1_returns_no_press_with_no_select() {
        let mut matrix = JoypadMatrix::new();
        matrix.write_p1(0x30); // both select bits high → Neither
        // Neither group selected → input nibble all 1s.
        assert_eq!(matrix.read_p1() & 0x0F, 0x0F);
    }

    #[test]
    fn pressing_a_with_action_group_selected_pulls_bit_low() {
        let mut matrix = JoypadMatrix::new();
        matrix.write_p1(0x10); // bit 5 = 0 → Action
        matrix.set(JoypadButton::A, true);
        assert_eq!(matrix.read_p1() & 0x0F, 0b1110); // bit 0 low
    }

    #[test]
    fn pressing_a_with_direction_group_selected_does_not_show() {
        let mut matrix = JoypadMatrix::new();
        matrix.write_p1(0x20); // bit 4 = 0 → Direction
        matrix.set(JoypadButton::A, true);
        assert_eq!(matrix.read_p1() & 0x0F, 0x0F); // A is masked out
    }

    #[test]
    fn select_both_combines_groups_via_and_of_pressed_state() {
        let mut matrix = JoypadMatrix::new();
        matrix.write_p1(0x00); // both select bits 0 → Both
        matrix.set(JoypadButton::A, true);
        matrix.set(JoypadButton::Up, true);
        // A is in Action.bit0, Up is in Direction.bit2.
        assert_eq!(matrix.read_p1() & 0x0F, 0b1010);
    }

    #[test]
    fn write_p1_preserves_high_bits_as_one() {
        let mut matrix = JoypadMatrix::new();
        matrix.write_p1(0x10);
        assert_eq!(matrix.read_p1() & 0xC0, 0xC0, "bits 6-7 wired high");
    }

    #[test]
    fn any_selected_pressed_tracks_active_group_only() {
        let mut matrix = JoypadMatrix::new();
        matrix.set(JoypadButton::A, true);
        matrix.write_p1(0x20); // direction selected
        assert!(!matrix.any_selected_pressed());
        matrix.write_p1(0x10); // action selected
        assert!(matrix.any_selected_pressed());
    }

    #[test]
    fn release_clears_button_bit() {
        let mut matrix = JoypadMatrix::new();
        matrix.set(JoypadButton::Down, true);
        assert!(matrix.is_pressed(JoypadButton::Down));
        matrix.set(JoypadButton::Down, false);
        assert!(!matrix.is_pressed(JoypadButton::Down));
    }
}
