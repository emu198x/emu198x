//! Keyboard handshake protocol.
//!
//! The Amiga keyboard controller sends keycodes as serial data through
//! CIA-A's serial port. The handshake works:
//! 1. Keyboard sends power-up codes (0xFD = initiate, 0xFE = terminate)
//! 2. Host pulses CIA-A PRA bit 6 (active low) to acknowledge
//! 3. Keyboard can then send one keycode per handshake
//!
//! Keycodes are 7-bit with bit 7 = key-up flag.

use std::collections::VecDeque;

/// Keyboard controller state.
pub struct Keyboard {
    /// Pending bytes to send (raw keycodes including boot codes).
    queue: VecDeque<u8>,
    /// Boot power-up codes pending.
    boot_pending: bool,
    /// Last CIA-A PRA output for handshake detection.
    last_pra: u8,
    /// Keyboard may send one byte after handshake.
    can_send: bool,
}

impl Keyboard {
    #[must_use]
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            boot_pending: true,
            last_pra: 0xFF,
            can_send: false,
        }
    }

    /// Reset keyboard state.
    pub fn reset(&mut self) {
        self.queue.clear();
        self.boot_pending = true;
        self.last_pra = 0xFF;
        self.can_send = false;
    }

    /// Queue a raw keycode (press = code, release = code | 0x80).
    pub fn queue_raw(&mut self, code: u8, pressed: bool) {
        let value = if pressed { code } else { code | 0x80 };
        self.queue.push_back(value);
    }

    /// Called when CIA-A PRA is written. Detects handshake toggle on bit 6.
    ///
    /// Returns the next byte to inject into SERDATR, if ready.
    pub fn cia_pra_written(&mut self, pra_output: u8) -> Option<u8> {
        let toggled = (pra_output ^ self.last_pra) & 0x02 != 0;
        self.last_pra = pra_output;

        if toggled {
            self.can_send = true;

            if self.boot_pending {
                self.boot_pending = false;
                // Queue boot power-up codes
                self.queue.push_front(0xFE); // terminate
                self.queue.push_front(0xFD); // initiate
            }

            return self.try_send();
        }

        None
    }

    /// Try to send one pending byte.
    pub fn try_send(&mut self) -> Option<u8> {
        if self.can_send {
            if let Some(byte) = self.queue.pop_front() {
                self.can_send = false;
                return Some(byte);
            }
        }
        None
    }

    /// Number of pending bytes in the queue.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.queue.len()
    }
}

impl Default for Keyboard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_codes_on_first_handshake() {
        let mut kbd = Keyboard::new();
        // Simulate PRA toggle on bit 1
        let byte = kbd.cia_pra_written(0xFD); // toggles bit 1
        assert_eq!(byte, Some(0xFD)); // initiate code
    }

    #[test]
    fn queued_keys_after_handshake() {
        let mut kbd = Keyboard::new();
        kbd.boot_pending = false;
        kbd.queue_raw(0x45, true); // ESC press

        let byte = kbd.cia_pra_written(0xFD);
        assert_eq!(byte, Some(0x45));
    }

    #[test]
    fn release_code_has_bit_7() {
        let mut kbd = Keyboard::new();
        kbd.queue_raw(0x45, false);
        assert_eq!(kbd.queue.front(), Some(&0xC5));
    }
}
