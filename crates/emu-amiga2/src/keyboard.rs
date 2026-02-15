//! Keyboard handshake protocol.
//!
//! The Amiga keyboard controller sends keycodes as serial data through
//! CIA-A's serial shift register (SDR). The protocol:
//!
//! 1. After power-on (~500ms delay), keyboard sends $FD (initiate) then $FE (terminate)
//! 2. CPU reads CIA-A SDR to get the byte
//! 3. CPU sets CIA-A CRA bit 6 = 1 (output mode → pulls KDAT low)
//! 4. After ~85µs, CPU clears CRA bit 6 = 0 (input mode → releases KDAT)
//! 5. Keyboard sees handshake complete, sends next byte
//!
//! Keycodes are 7-bit with bit 7 = key-up flag.

use std::collections::VecDeque;

/// E-clock ticks before keyboard sends power-up codes (~100ms at 709 kHz).
const POWER_UP_DELAY_TICKS: u32 = 70_000;

/// Keyboard controller state.
pub struct Keyboard {
    /// Pending bytes to send (raw keycodes including boot codes).
    queue: VecDeque<u8>,
    /// Power-up delay counter (E-clock ticks remaining before first send).
    power_up_delay: u32,
    /// Boot power-up codes already queued.
    boot_sent: bool,
    /// Previous CRA bit 6 state for handshake edge detection.
    last_cra_bit6: bool,
    /// Keyboard may send one byte (set after handshake or initial power-up).
    can_send: bool,
}

impl Keyboard {
    #[must_use]
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            power_up_delay: POWER_UP_DELAY_TICKS,
            boot_sent: false,
            last_cra_bit6: false,
            can_send: false,
        }
    }

    /// Reset keyboard state.
    pub fn reset(&mut self) {
        self.queue.clear();
        self.power_up_delay = POWER_UP_DELAY_TICKS;
        self.boot_sent = false;
        self.last_cra_bit6 = false;
        self.can_send = false;
    }

    /// Queue a raw keycode (press = code, release = code | 0x80).
    pub fn queue_raw(&mut self, code: u8, pressed: bool) {
        let value = if pressed { code } else { code | 0x80 };
        self.queue.push_back(value);
    }

    /// Called every E-clock tick. Handles power-up delay.
    /// Returns a byte to inject into CIA-A SDR if ready.
    pub fn pump(&mut self) -> Option<u8> {
        // Count down power-up delay
        if self.power_up_delay > 0 {
            self.power_up_delay -= 1;
            if self.power_up_delay == 0 {
                // Queue boot power-up codes
                self.queue.push_front(0xFE); // terminate
                self.queue.push_front(0xFD); // initiate
                self.can_send = true;
                self.boot_sent = true;
                // Fall through to try_send — send first byte on the same tick
            } else {
                return None;
            }
        }

        self.try_send()
    }

    /// Called when CIA-A CRA is written. Detects handshake on bit 6.
    ///
    /// Handshake = CRA bit 6 goes from 1 (output/KDAT low) back to 0 (input/released).
    /// After handshake, the keyboard may send the next byte.
    pub fn cia_cra_written(&mut self, cra: u8) {
        let bit6 = cra & 0x40 != 0;
        let was_output = self.last_cra_bit6;
        self.last_cra_bit6 = bit6;

        // Falling edge on bit 6: output → input = handshake complete
        if was_output && !bit6 {
            self.can_send = true;
        }
    }

    /// Try to send one pending byte.
    fn try_send(&mut self) -> Option<u8> {
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
    fn boot_codes_after_power_up_delay() {
        let mut kbd = Keyboard::new();
        kbd.power_up_delay = 3; // Short delay for testing

        // Ticks before delay expires: no data
        assert_eq!(kbd.pump(), None);
        assert_eq!(kbd.pump(), None);

        // Third tick: delay expires, boot codes queued, first byte sent
        assert_eq!(kbd.pump(), Some(0xFD));

        // Need handshake before second byte
        assert_eq!(kbd.pump(), None);

        // Simulate handshake: CRA bit 6 goes 1 then 0
        kbd.cia_cra_written(0x40); // bit 6 = 1 (output mode)
        kbd.cia_cra_written(0x00); // bit 6 = 0 (input mode) → handshake

        assert_eq!(kbd.pump(), Some(0xFE));
    }

    #[test]
    fn queued_keys_after_handshake() {
        let mut kbd = Keyboard::new();
        kbd.power_up_delay = 0;
        kbd.boot_sent = true;
        kbd.queue_raw(0x45, true); // ESC press

        // Not allowed to send until handshake
        assert_eq!(kbd.pump(), None);

        // Simulate handshake
        kbd.cia_cra_written(0x40);
        kbd.cia_cra_written(0x00);
        assert_eq!(kbd.pump(), Some(0x45));
    }

    #[test]
    fn release_code_has_bit_7() {
        let mut kbd = Keyboard::new();
        kbd.queue_raw(0x45, false);
        assert_eq!(kbd.queue.front(), Some(&0xC5));
    }

    #[test]
    fn reset_clears_state() {
        let mut kbd = Keyboard::new();
        kbd.power_up_delay = 0;
        kbd.can_send = true;
        kbd.queue.push_back(0x42);
        kbd.reset();
        assert_eq!(kbd.power_up_delay, POWER_UP_DELAY_TICKS);
        assert!(!kbd.can_send);
        assert!(kbd.queue.is_empty());
    }
}
