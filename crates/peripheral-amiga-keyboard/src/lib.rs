//! Amiga keyboard controller emulator.
//!
//! The real Amiga keyboard contains a 6500/1 microprocessor that handles
//! key scanning and communication with the host. It sends bytes serially
//! via CIA-A's SP/CNT lines. This module models the keyboard's state
//! machine at a functional level, producing bytes at E-clock rate.
//!
//! Power-up sequence: the keyboard sends $FD (init power-up) then $FE
//! (terminate power-up), each requiring a handshake from the host.

use std::collections::VecDeque;

/// E-clock ticks before power-up sequence begins (~200ms at 709 kHz).
const POWERUP_DELAY_TICKS: u32 = 150_000;

/// E-clock ticks between transmitted bytes (~1ms at 709 kHz).
const BYTE_INTERVAL_TICKS: u32 = 700;

/// E-clock ticks to wait for handshake before resending (~143ms).
const HANDSHAKE_TIMEOUT_TICKS: u32 = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Waiting for initial power-up delay.
    PowerUpDelay,
    /// Sending $FD (init power-up stream).
    SendInitPowerUp,
    /// Waiting for host handshake after $FD.
    WaitHandshakeInit,
    /// Sending $FE (terminate power-up stream).
    SendTermPowerUp,
    /// Waiting for host handshake after $FE.
    WaitHandshakeTerm,
    /// Idle: ready to send queued key events.
    Idle,
    /// A key byte was just sent, waiting for handshake.
    WaitHandshakeKey,
}

pub struct AmigaKeyboard {
    state: State,
    timer: u32,
    key_queue: VecDeque<u8>,
}

impl AmigaKeyboard {
    pub fn new() -> Self {
        Self {
            state: State::PowerUpDelay,
            timer: 0,
            key_queue: VecDeque::new(),
        }
    }

    /// Tick at E-clock rate (~709 kHz). Returns `Some(byte)` when a
    /// rotated keycode is ready to inject into CIA-A SDR.
    pub fn tick(&mut self) -> Option<u8> {
        self.timer = self.timer.saturating_add(1);
        match self.state {
            State::PowerUpDelay => {
                if self.timer >= POWERUP_DELAY_TICKS {
                    self.state = State::SendInitPowerUp;
                    self.timer = 0;
                }
                None
            }
            State::SendInitPowerUp => {
                self.state = State::WaitHandshakeInit;
                self.timer = 0;
                Some(rotate_byte(0xFD))
            }
            State::WaitHandshakeInit => {
                if self.timer >= HANDSHAKE_TIMEOUT_TICKS {
                    // Timeout: resend
                    self.state = State::SendInitPowerUp;
                    self.timer = 0;
                }
                None
            }
            State::SendTermPowerUp => {
                self.state = State::WaitHandshakeTerm;
                self.timer = 0;
                Some(rotate_byte(0xFE))
            }
            State::WaitHandshakeTerm => {
                if self.timer >= HANDSHAKE_TIMEOUT_TICKS {
                    self.state = State::SendTermPowerUp;
                    self.timer = 0;
                }
                None
            }
            State::Idle => {
                if self.timer >= BYTE_INTERVAL_TICKS {
                    if let Some(byte) = self.key_queue.pop_front() {
                        self.state = State::WaitHandshakeKey;
                        self.timer = 0;
                        return Some(rotate_byte(byte));
                    }
                }
                None
            }
            State::WaitHandshakeKey => {
                if self.timer >= HANDSHAKE_TIMEOUT_TICKS {
                    // Timeout: resend by re-queuing would be complex; just go idle
                    self.state = State::Idle;
                    self.timer = 0;
                }
                None
            }
        }
    }

    /// Host acknowledged the last byte (CIA-A CRA bit 6 set to output mode).
    pub fn handshake(&mut self) {
        match self.state {
            State::WaitHandshakeInit => {
                self.state = State::SendTermPowerUp;
                self.timer = 0;
            }
            State::WaitHandshakeTerm => {
                self.state = State::Idle;
                self.timer = 0;
            }
            State::WaitHandshakeKey => {
                self.state = State::Idle;
                self.timer = 0;
            }
            _ => {}
        }
    }

    /// Queue a key event. The raw keycode has bit 7 clear for key-down,
    /// bit 7 set for key-up.
    pub fn key_event(&mut self, keycode: u8, pressed: bool) {
        let byte = if pressed { keycode & 0x7F } else { keycode | 0x80 };
        self.key_queue.push_back(byte);
    }
}

impl Default for AmigaKeyboard {
    fn default() -> Self {
        Self::new()
    }
}

/// Amiga keycodes are rotated left 1 bit before serial transmission.
/// The ROM's keyboard interrupt handler rotates right to recover.
fn rotate_byte(byte: u8) -> u8 {
    (byte << 1) | (byte >> 7)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_up_sequence() {
        let mut kb = AmigaKeyboard::new();

        // Tick through power-up delay — no output
        for _ in 0..POWERUP_DELAY_TICKS - 1 {
            assert_eq!(kb.tick(), None);
        }

        // The tick that hits the delay threshold transitions state;
        // the NEXT tick sends $FD
        assert_eq!(kb.tick(), None); // transitions to SendInitPowerUp
        let byte = kb.tick();       // sends $FD
        assert_eq!(byte, Some(rotate_byte(0xFD)));

        // Now waiting for handshake — no output
        assert_eq!(kb.tick(), None);

        // Handshake → sends $FE
        kb.handshake();
        let byte = kb.tick();
        assert_eq!(byte, Some(rotate_byte(0xFE)));

        // Handshake → idle
        kb.handshake();
        assert_eq!(kb.state, State::Idle);
    }

    #[test]
    fn key_event_after_powerup() {
        let mut kb = AmigaKeyboard::new();

        // Fast-forward through power-up
        for _ in 0..POWERUP_DELAY_TICKS + 1 {
            kb.tick();
        }
        kb.tick(); // sends $FD
        kb.handshake();
        kb.tick(); // sends $FE
        kb.handshake();

        // Queue a key press (keycode $45 = Enter)
        kb.key_event(0x45, true);

        // Wait for byte interval minus one — no output yet
        for _ in 0..BYTE_INTERVAL_TICKS - 1 {
            assert_eq!(kb.tick(), None);
        }

        // The tick that hits the interval sends the byte
        let byte = kb.tick();
        assert_eq!(byte, Some(rotate_byte(0x45)));

        // Handshake completes
        kb.handshake();
        assert_eq!(kb.state, State::Idle);
    }

    #[test]
    fn key_release_has_bit7_set() {
        let mut kb = AmigaKeyboard::new();
        kb.key_event(0x45, false);
        // The queued byte should have bit 7 set
        assert_eq!(kb.key_queue.front(), Some(&0xC5));
    }

    #[test]
    fn rotate_byte_round_trip() {
        for byte in 0..=255u8 {
            let rotated = rotate_byte(byte);
            let recovered = (rotated >> 1) | (rotated << 7);
            assert_eq!(recovered, byte);
        }
    }
}
