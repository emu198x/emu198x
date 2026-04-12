//! ZX Spectrum 48K `$FE` port state.
//!
//! Source references:
//! - `wiki/systems/spectrum/variants.md`
//! - Adapted from `/Users/stevehill/Projects/Emu198x-Older/crates/ferranti-ula-6c001e/src/lib.rs`
//! - Adapted from `/Users/stevehill/Projects/Emu198x-Older/crates/common-sinclair-zx-spectrum/src/ula_engine.rs`

use crate::keyboard::KeyboardMatrix;
use crate::machine::BoardIssue;

const FE_HIGH_BITS: u8 = 0xa0;

/// External tape EAR line state visible to port `$FE`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TapeInput {
    connected: bool,
    level: bool,
}

impl TapeInput {
    /// Creates a disconnected tape input line.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns whether the tape signal is connected.
    #[must_use]
    pub const fn connected(self) -> bool {
        self.connected
    }

    /// Returns the current tape EAR level.
    #[must_use]
    pub const fn level(self) -> bool {
        self.level
    }

    /// Sets whether the tape signal is connected.
    pub fn set_connected(&mut self, connected: bool) {
        self.connected = connected;
    }

    /// Sets the current tape EAR level.
    pub fn set_level(&mut self, level: bool) {
        self.level = level;
    }
}

/// Latched `$FE` output state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortFeState {
    border_color: u8,
    mic_high: bool,
    beeper_high: bool,
}

impl PortFeState {
    /// Creates the deterministic power-on `$FE` latch state.
    ///
    /// The older ULA engine initialised the border to white; this machine-side
    /// crate preserves that observable startup state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            border_color: 7,
            mic_high: false,
            beeper_high: false,
        }
    }

    /// Returns the current border colour.
    #[must_use]
    pub const fn border_color(self) -> u8 {
        self.border_color
    }

    /// Returns the current MIC output state.
    #[must_use]
    pub const fn mic_high(self) -> bool {
        self.mic_high
    }

    /// Returns the current beeper output state.
    #[must_use]
    pub const fn beeper_high(self) -> bool {
        self.beeper_high
    }

    /// Applies one write to port `$FE`.
    pub fn write(&mut self, value: u8) {
        self.border_color = value & 0x07;
        self.mic_high = value & 0x08 != 0;
        self.beeper_high = value & 0x10 != 0;
    }

    /// Reads port `$FE` for the selected keyboard half-rows and EAR source.
    #[must_use]
    pub fn read(
        self,
        issue: BoardIssue,
        keyboard: &KeyboardMatrix,
        tape_input: TapeInput,
        port: u16,
    ) -> u8 {
        FE_HIGH_BITS | keyboard.scan_port(port) | self.ear_bit(issue, tape_input)
    }

    const fn ear_feedback_bit(self, issue: BoardIssue) -> u8 {
        let high = match issue {
            BoardIssue::Issue2 => self.mic_high || self.beeper_high,
            BoardIssue::Issue3 => self.beeper_high,
        };

        if high { 0x40 } else { 0x00 }
    }

    const fn ear_bit(self, issue: BoardIssue, tape_input: TapeInput) -> u8 {
        if tape_input.connected() {
            if tape_input.level() { 0x40 } else { 0x00 }
        } else {
            self.ear_feedback_bit(issue)
        }
    }
}

impl Default for PortFeState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard::SpectrumKey;

    #[test]
    fn write_tracks_border_mic_and_beeper_bits() {
        let mut port = PortFeState::new();
        port.write(0x1d);

        assert_eq!(port.border_color(), 5);
        assert!(port.mic_high());
        assert!(port.beeper_high());
    }

    #[test]
    fn issue2_and_issue3_differ_on_mic_only_feedback() {
        let keyboard = KeyboardMatrix::new();
        let tape = TapeInput::new();
        let mut port = PortFeState::new();
        port.write(0x08);

        assert_eq!(
            port.read(BoardIssue::Issue2, &keyboard, tape, 0xfffe) & 0x40,
            0x40
        );
        assert_eq!(
            port.read(BoardIssue::Issue3, &keyboard, tape, 0xfffe) & 0x40,
            0x00
        );
    }

    #[test]
    fn connected_tape_overrides_feedback() {
        let keyboard = KeyboardMatrix::new();
        let mut tape = TapeInput::new();
        tape.set_connected(true);

        let mut port = PortFeState::new();
        port.write(0x10);

        tape.set_level(false);
        assert_eq!(
            port.read(BoardIssue::Issue3, &keyboard, tape, 0xfffe) & 0x40,
            0x00
        );

        tape.set_level(true);
        assert_eq!(
            port.read(BoardIssue::Issue3, &keyboard, tape, 0xfffe) & 0x40,
            0x40
        );
    }

    #[test]
    fn read_combines_keyboard_and_ear_bits() {
        let mut keyboard = KeyboardMatrix::new();
        keyboard.press_key(SpectrumKey::CapsShift);
        let mut port = PortFeState::new();
        port.write(0x10);

        assert_eq!(
            port.read(BoardIssue::Issue3, &keyboard, TapeInput::new(), 0xfefe),
            0xfe
        );
    }
}
