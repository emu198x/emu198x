//! NES controller (joypad) emulation.
//!
//! The NES controller is a serial shift register accessed via $4016/$4017.
//! Writing bit 0 = 1 to $4016 enables strobe (continuously reloads).
//! Writing bit 0 = 0 latches the current button state.
//! Each read returns one bit and shifts the register right.

/// NES button indices (bit positions).
pub mod button {
    pub const A: u8 = 0;
    pub const B: u8 = 1;
    pub const SELECT: u8 = 2;
    pub const START: u8 = 3;
    pub const UP: u8 = 4;
    pub const DOWN: u8 = 5;
    pub const LEFT: u8 = 6;
    pub const RIGHT: u8 = 7;
}

/// NES controller state.
pub struct Controller {
    /// Current button state (bit per button).
    buttons: u8,
    /// Latched shift register.
    shift_register: u8,
    /// Strobe mode: when true, shift register continuously reloads.
    strobe: bool,
}

impl Controller {
    #[must_use]
    pub fn new() -> Self {
        Self {
            buttons: 0,
            shift_register: 0,
            strobe: false,
        }
    }

    /// Set a button state (true = pressed).
    pub fn set_button(&mut self, button: u8, pressed: bool) {
        if pressed {
            self.buttons |= 1 << button;
        } else {
            self.buttons &= !(1 << button);
        }
        // If strobe is active, keep shift register updated
        if self.strobe {
            self.shift_register = self.buttons;
        }
    }

    /// Read $4016/$4017: return bit 0 of shift register, shift right.
    pub fn read(&mut self) -> u8 {
        if self.strobe {
            // In strobe mode, always return button A state
            return self.buttons & 1;
        }
        let result = self.shift_register & 1;
        self.shift_register >>= 1;
        // After all 8 bits are shifted out, reads return 1
        self.shift_register |= 0x80;
        result
    }

    /// Write $4016: bit 0 controls strobe.
    pub fn write(&mut self, value: u8) {
        let new_strobe = value & 1 != 0;
        if self.strobe && !new_strobe {
            // Falling edge: latch current buttons into shift register
            self.shift_register = self.buttons;
        }
        self.strobe = new_strobe;
    }

    /// Current button state byte (for observation).
    #[must_use]
    pub fn buttons(&self) -> u8 {
        self.buttons
    }
}

impl Default for Controller {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strobe_latch_and_read() {
        let mut c = Controller::new();
        c.set_button(button::A, true);
        c.set_button(button::START, true);

        // Enable then disable strobe to latch
        c.write(1);
        c.write(0);

        // Read all 8 bits
        assert_eq!(c.read(), 1); // A
        assert_eq!(c.read(), 0); // B
        assert_eq!(c.read(), 0); // Select
        assert_eq!(c.read(), 1); // Start
        assert_eq!(c.read(), 0); // Up
        assert_eq!(c.read(), 0); // Down
        assert_eq!(c.read(), 0); // Left
        assert_eq!(c.read(), 0); // Right
        // After 8 reads, should return 1
        assert_eq!(c.read(), 1);
    }

    #[test]
    fn strobe_mode_returns_a_button() {
        let mut c = Controller::new();
        c.set_button(button::A, true);
        c.write(1); // Strobe on

        // In strobe mode, always returns A button state
        assert_eq!(c.read(), 1);
        assert_eq!(c.read(), 1);

        c.set_button(button::A, false);
        assert_eq!(c.read(), 0);
    }

    #[test]
    fn buttons_byte() {
        let mut c = Controller::new();
        c.set_button(button::A, true);
        c.set_button(button::B, true);
        assert_eq!(c.buttons(), 0x03);
    }
}
