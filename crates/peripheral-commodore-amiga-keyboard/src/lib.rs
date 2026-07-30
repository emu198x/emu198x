//! Amiga keyboard controller emulator.
//!
//! The real Amiga keyboard contains a 6500/1 microprocessor that handles
//! key scanning and communication with the host. It sends bytes serially
//! via CIA-A's SP/CNT lines. This module models the keyboard's state
//! machine at a functional level, producing bytes at E-clock rate.
//!
//! Power-up sequence: the keyboard sends $FD (init power-up) then $FE
//! (terminate power-up), each requiring a handshake from the host.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// E-clock ticks before power-up sequence begins (~200ms at 709 kHz).
const POWERUP_DELAY_TICKS: u32 = 150_000;

/// E-clock ticks between transmitted bytes (~1ms at 709 kHz).
const BYTE_INTERVAL_TICKS: u32 = 700;

/// E-clock ticks to wait for handshake before resending (~143ms).
const HANDSHAKE_TIMEOUT_TICKS: u32 = 100_000;

/// Functional protocol state retained by the keyboard controller.
///
/// The current model delivers complete encoded bytes to CIA-A from
/// [`AmigaKeyboard::tick`]. It does not retain an intermediate bit-serial
/// shifter state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AmigaKeyboardProtocolState {
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

impl AmigaKeyboardProtocolState {
    fn name(self) -> &'static str {
        match self {
            Self::PowerUpDelay => "PowerUpDelay",
            Self::SendInitPowerUp => "SendInitPowerUp",
            Self::WaitHandshakeInit => "WaitHandshakeInit",
            Self::SendTermPowerUp => "SendTermPowerUp",
            Self::WaitHandshakeTerm => "WaitHandshakeTerm",
            Self::Idle => "Idle",
            Self::WaitHandshakeKey => "WaitHandshakeKey",
        }
    }

    fn timer_limit_ticks(self) -> Option<u32> {
        match self {
            Self::PowerUpDelay => Some(POWERUP_DELAY_TICKS),
            Self::WaitHandshakeInit | Self::WaitHandshakeTerm | Self::WaitHandshakeKey => {
                Some(HANDSHAKE_TIMEOUT_TICKS)
            }
            Self::Idle => Some(BYTE_INTERVAL_TICKS),
            Self::SendInitPowerUp | Self::SendTermPowerUp => None,
        }
    }
}

/// Side-effect-free view of the keyboard's complete implemented protocol state.
///
/// Bytes are reported before the keyboard's rotate-and-invert wire encoding and
/// in their encoded form. The queue is copied in transmission order so
/// debuggers can inspect it without consuming an event.
///
/// The controller is currently a functional byte-level model. Consequently,
/// [`Self::serial_bit_index`] is always `None`: when [`Self::current_byte`] is
/// present, all eight bits have already been delivered atomically and the
/// controller is waiting for the host handshake.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmigaKeyboardDiagnosticSnapshot {
    /// Current protocol state.
    pub state: AmigaKeyboardProtocolState,
    /// Elapsed E-clock ticks in the current protocol state.
    pub timer_ticks: u32,
    /// Tick threshold relevant to the current state, if it has one.
    pub timer_limit_ticks: Option<u32>,
    /// Saturating number of ticks remaining before the current threshold.
    pub timer_remaining_ticks: Option<u32>,
    /// Whether the current state's timer has reached its threshold.
    pub timer_expired: bool,
    /// Raw byte most recently delivered and currently awaiting a handshake.
    pub current_byte: Option<u8>,
    /// Rotate-and-invert encoding of [`Self::current_byte`].
    pub current_encoded_byte: Option<u8>,
    /// Raw byte the state machine will next deliver once it may transmit.
    pub pending_byte: Option<u8>,
    /// Rotate-and-invert encoding of [`Self::pending_byte`].
    pub pending_encoded_byte: Option<u8>,
    /// Index of the next bit in an active bit-serial transfer.
    ///
    /// This is `None` because the implemented model delivers whole bytes.
    pub serial_bit_index: Option<u8>,
    /// Number of bits already delivered for [`Self::current_byte`].
    ///
    /// This is `Some(8)` while a delivered byte awaits its handshake and
    /// `None` when no byte is in flight.
    pub serial_bits_completed: Option<u8>,
    /// Number of bits remaining for [`Self::current_byte`].
    ///
    /// This is `Some(0)` while a delivered byte awaits its handshake and
    /// `None` when no byte is in flight.
    pub serial_bits_remaining: Option<u8>,
    /// Whether an intermediate bit-serial transfer is active.
    ///
    /// This remains false in the current byte-level model.
    pub serial_transfer_active: bool,
    /// Whether the implementation delivers each encoded byte atomically.
    pub atomic_byte_delivery: bool,
    /// Whether the power-up/reset protocol has not yet completed.
    pub reset_sequence_active: bool,
    /// Whether the keyboard has completed the power-up/reset protocol.
    pub reset_sequence_complete: bool,
    /// Whether the controller is waiting for the host to acknowledge a byte.
    pub waiting_for_handshake: bool,
    /// Whether an idle controller is waiting for the inter-byte delay.
    pub waiting_for_byte_interval: bool,
    /// Whether the current handshake timeout will resend its byte.
    pub timeout_will_resend: bool,
    /// Whether the current handshake timeout will drop its key byte.
    pub timeout_will_drop: bool,
    /// Whether the next call to [`AmigaKeyboard::tick`] can deliver a byte.
    pub transmission_ready: bool,
    /// Queued raw key-event bytes in transmission order.
    pub queued_bytes: Vec<u8>,
    /// Number of queued raw key-event bytes.
    pub queue_count: usize,
    /// Current allocated capacity of the unbounded software event queue.
    pub queue_allocated_capacity: usize,
    /// Whether the event queue is empty.
    pub queue_is_empty: bool,
    /// Total number of encoded bytes delivered to the host.
    pub bytes_sent: u32,
}

/// Functional model of the Amiga keyboard controller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmigaKeyboard {
    state: AmigaKeyboardProtocolState,
    timer: u32,
    /// Raw byte delivered by the last transmission and awaiting handshake.
    #[serde(default)]
    current_byte: Option<u8>,
    key_queue: VecDeque<u8>,
    /// Total number of bytes sent to the host (for diagnostics).
    pub bytes_sent: u32,
}

impl AmigaKeyboard {
    pub fn new() -> Self {
        Self {
            state: AmigaKeyboardProtocolState::PowerUpDelay,
            timer: 0,
            current_byte: None,
            key_queue: VecDeque::new(),
            bytes_sent: 0,
        }
    }

    /// Tick at E-clock rate (~709 kHz). Returns `Some(byte)` when a
    /// rotated keycode is ready to inject into CIA-A SDR.
    pub fn tick(&mut self) -> Option<u8> {
        self.timer = self.timer.saturating_add(1);
        match self.state {
            AmigaKeyboardProtocolState::PowerUpDelay => {
                if self.timer >= POWERUP_DELAY_TICKS {
                    self.state = AmigaKeyboardProtocolState::SendInitPowerUp;
                    self.timer = 0;
                }
                None
            }
            AmigaKeyboardProtocolState::SendInitPowerUp => {
                let byte = 0xFD;
                self.current_byte = Some(byte);
                self.state = AmigaKeyboardProtocolState::WaitHandshakeInit;
                self.timer = 0;
                self.bytes_sent += 1;
                Some(encode_keycode(byte))
            }
            AmigaKeyboardProtocolState::WaitHandshakeInit => {
                if self.timer >= HANDSHAKE_TIMEOUT_TICKS {
                    // Timeout: resend
                    self.current_byte = None;
                    self.state = AmigaKeyboardProtocolState::SendInitPowerUp;
                    self.timer = 0;
                }
                None
            }
            AmigaKeyboardProtocolState::SendTermPowerUp => {
                let byte = 0xFE;
                self.current_byte = Some(byte);
                self.state = AmigaKeyboardProtocolState::WaitHandshakeTerm;
                self.timer = 0;
                self.bytes_sent += 1;
                Some(encode_keycode(byte))
            }
            AmigaKeyboardProtocolState::WaitHandshakeTerm => {
                if self.timer >= HANDSHAKE_TIMEOUT_TICKS {
                    self.current_byte = None;
                    self.state = AmigaKeyboardProtocolState::SendTermPowerUp;
                    self.timer = 0;
                }
                None
            }
            AmigaKeyboardProtocolState::Idle => {
                if self.timer >= BYTE_INTERVAL_TICKS
                    && let Some(byte) = self.key_queue.pop_front()
                {
                    self.current_byte = Some(byte);
                    self.state = AmigaKeyboardProtocolState::WaitHandshakeKey;
                    self.timer = 0;
                    self.bytes_sent += 1;
                    return Some(encode_keycode(byte));
                }
                None
            }
            AmigaKeyboardProtocolState::WaitHandshakeKey => {
                if self.timer >= HANDSHAKE_TIMEOUT_TICKS {
                    // Timeout: resend by re-queuing would be complex; just go idle
                    self.current_byte = None;
                    self.state = AmigaKeyboardProtocolState::Idle;
                    self.timer = 0;
                }
                None
            }
        }
    }

    /// Host acknowledged the last byte (CIA-A CRA bit 6 set to output mode).
    pub fn handshake(&mut self) {
        match self.state {
            AmigaKeyboardProtocolState::WaitHandshakeInit => {
                self.current_byte = None;
                self.state = AmigaKeyboardProtocolState::SendTermPowerUp;
                self.timer = 0;
            }
            AmigaKeyboardProtocolState::WaitHandshakeTerm => {
                self.current_byte = None;
                self.state = AmigaKeyboardProtocolState::Idle;
                self.timer = 0;
            }
            AmigaKeyboardProtocolState::WaitHandshakeKey => {
                self.current_byte = None;
                self.state = AmigaKeyboardProtocolState::Idle;
                self.timer = 0;
            }
            _ => {}
        }
    }

    /// Queue a key event. The raw keycode has bit 7 clear for key-down,
    /// bit 7 set for key-up.
    pub fn key_event(&mut self, keycode: u8, pressed: bool) {
        let byte = if pressed {
            keycode & 0x7F
        } else {
            keycode | 0x80
        };
        self.key_queue.push_back(byte);
    }

    #[must_use]
    pub fn debug_state_name(&self) -> &'static str {
        self.state.name()
    }

    #[must_use]
    pub fn debug_timer(&self) -> u32 {
        self.timer
    }

    #[must_use]
    pub fn queued_key_count(&self) -> usize {
        self.key_queue.len()
    }

    /// Return a side-effect-free snapshot of all implemented protocol state.
    #[must_use]
    pub fn diagnostic_snapshot(&self) -> AmigaKeyboardDiagnosticSnapshot {
        let timer_limit_ticks = self.state.timer_limit_ticks();
        let pending_byte = match self.state {
            AmigaKeyboardProtocolState::PowerUpDelay
            | AmigaKeyboardProtocolState::SendInitPowerUp => Some(0xFD),
            AmigaKeyboardProtocolState::WaitHandshakeInit
            | AmigaKeyboardProtocolState::SendTermPowerUp => Some(0xFE),
            AmigaKeyboardProtocolState::WaitHandshakeTerm
            | AmigaKeyboardProtocolState::Idle
            | AmigaKeyboardProtocolState::WaitHandshakeKey => self.key_queue.front().copied(),
        };
        let waiting_for_handshake = matches!(
            self.state,
            AmigaKeyboardProtocolState::WaitHandshakeInit
                | AmigaKeyboardProtocolState::WaitHandshakeTerm
                | AmigaKeyboardProtocolState::WaitHandshakeKey
        );
        let reset_sequence_active = !matches!(
            self.state,
            AmigaKeyboardProtocolState::Idle | AmigaKeyboardProtocolState::WaitHandshakeKey
        );
        let queue_count = self.key_queue.len();

        AmigaKeyboardDiagnosticSnapshot {
            state: self.state,
            timer_ticks: self.timer,
            timer_limit_ticks,
            timer_remaining_ticks: timer_limit_ticks.map(|limit| limit.saturating_sub(self.timer)),
            timer_expired: timer_limit_ticks.is_some_and(|limit| self.timer >= limit),
            current_byte: self.current_byte,
            current_encoded_byte: self.current_byte.map(encode_keycode),
            pending_byte,
            pending_encoded_byte: pending_byte.map(encode_keycode),
            serial_bit_index: None,
            serial_bits_completed: self.current_byte.map(|_| 8),
            serial_bits_remaining: self.current_byte.map(|_| 0),
            serial_transfer_active: false,
            atomic_byte_delivery: true,
            reset_sequence_active,
            reset_sequence_complete: !reset_sequence_active,
            waiting_for_handshake,
            waiting_for_byte_interval: self.state == AmigaKeyboardProtocolState::Idle
                && self.timer < BYTE_INTERVAL_TICKS
                && !self.key_queue.is_empty(),
            timeout_will_resend: matches!(
                self.state,
                AmigaKeyboardProtocolState::WaitHandshakeInit
                    | AmigaKeyboardProtocolState::WaitHandshakeTerm
            ),
            timeout_will_drop: self.state == AmigaKeyboardProtocolState::WaitHandshakeKey,
            transmission_ready: matches!(
                self.state,
                AmigaKeyboardProtocolState::SendInitPowerUp
                    | AmigaKeyboardProtocolState::SendTermPowerUp
            ) || (self.state == AmigaKeyboardProtocolState::Idle
                && self.timer >= BYTE_INTERVAL_TICKS
                && !self.key_queue.is_empty()),
            queued_bytes: self.key_queue.iter().copied().collect(),
            queue_count,
            queue_allocated_capacity: self.key_queue.capacity(),
            queue_is_empty: queue_count == 0,
            bytes_sent: self.bytes_sent,
        }
    }
}

impl Default for AmigaKeyboard {
    fn default() -> Self {
        Self::new()
    }
}

/// Encode a keycode for CIA-A SDR transmission.
///
/// The Amiga keyboard rotates the keycode left by 1 bit before sending.
/// The KDAT line is active-low, so the CIA captures the inverse of each
/// bit. The ROM decodes by inverting then rotating right (or equivalently,
/// rotating right then inverting — the operations commute).
fn encode_keycode(byte: u8) -> u8 {
    !byte.rotate_left(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boot_keyboard_to_idle(kb: &mut AmigaKeyboard) {
        for _ in 0..POWERUP_DELAY_TICKS {
            assert_eq!(kb.tick(), None);
        }
        assert_eq!(kb.tick(), Some(encode_keycode(0xFD)));
        kb.handshake();
        assert_eq!(kb.tick(), Some(encode_keycode(0xFE)));
        kb.handshake();
        assert_eq!(kb.state, AmigaKeyboardProtocolState::Idle);
    }

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
        let byte = kb.tick(); // sends $FD
        assert_eq!(byte, Some(encode_keycode(0xFD)));

        // Now waiting for handshake — no output
        assert_eq!(kb.tick(), None);

        // Handshake → sends $FE
        kb.handshake();
        let byte = kb.tick();
        assert_eq!(byte, Some(encode_keycode(0xFE)));

        // Handshake → idle
        kb.handshake();
        assert_eq!(kb.state, AmigaKeyboardProtocolState::Idle);
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
        assert_eq!(byte, Some(encode_keycode(0x45)));

        // Handshake completes
        kb.handshake();
        assert_eq!(kb.state, AmigaKeyboardProtocolState::Idle);
    }

    #[test]
    fn key_release_has_bit7_set() {
        let mut kb = AmigaKeyboard::new();
        kb.key_event(0x45, false);
        // The queued byte should have bit 7 set
        assert_eq!(kb.key_queue.front(), Some(&0xC5));
    }

    #[test]
    fn encode_decode_round_trip() {
        // The ROM decodes by inverting then rotating right (or vice versa).
        for byte in 0..=255u8 {
            let encoded = encode_keycode(byte);
            let recovered = (!encoded).rotate_right(1);
            assert_eq!(recovered, byte);
        }
    }

    #[test]
    fn encode_matches_winuae() {
        // WinUAE: kbcode = ~((keycode << 1) | (keycode >> 7))
        // Our encode_keycode should produce the same value.
        for byte in 0..=255u8 {
            let winuae = !byte.rotate_left(1);
            assert_eq!(encode_keycode(byte), winuae);
        }
    }

    #[test]
    fn init_powerup_byte_resends_after_handshake_timeout() {
        let mut kb = AmigaKeyboard::new();

        for _ in 0..POWERUP_DELAY_TICKS {
            assert_eq!(kb.tick(), None);
        }
        assert_eq!(kb.tick(), Some(encode_keycode(0xFD)));
        assert_eq!(kb.state, AmigaKeyboardProtocolState::WaitHandshakeInit);

        for _ in 0..HANDSHAKE_TIMEOUT_TICKS {
            assert_eq!(kb.tick(), None);
        }

        assert_eq!(kb.state, AmigaKeyboardProtocolState::SendInitPowerUp);
        assert_eq!(kb.tick(), Some(encode_keycode(0xFD)));
        assert_eq!(kb.bytes_sent, 2);
    }

    #[test]
    fn terminate_powerup_byte_resends_after_handshake_timeout() {
        let mut kb = AmigaKeyboard::new();

        for _ in 0..POWERUP_DELAY_TICKS {
            assert_eq!(kb.tick(), None);
        }
        assert_eq!(kb.tick(), Some(encode_keycode(0xFD)));
        kb.handshake();
        assert_eq!(kb.tick(), Some(encode_keycode(0xFE)));
        assert_eq!(kb.state, AmigaKeyboardProtocolState::WaitHandshakeTerm);

        for _ in 0..HANDSHAKE_TIMEOUT_TICKS {
            assert_eq!(kb.tick(), None);
        }

        assert_eq!(kb.state, AmigaKeyboardProtocolState::SendTermPowerUp);
        assert_eq!(kb.tick(), Some(encode_keycode(0xFE)));
        assert_eq!(kb.bytes_sent, 3);
    }

    #[test]
    fn key_byte_timeout_returns_to_idle_after_dropping_in_flight_byte() {
        let mut kb = AmigaKeyboard::new();
        boot_keyboard_to_idle(&mut kb);

        kb.key_event(0x20, true);
        kb.key_event(0x21, true);

        for _ in 0..BYTE_INTERVAL_TICKS - 1 {
            assert_eq!(kb.tick(), None);
        }
        assert_eq!(kb.tick(), Some(encode_keycode(0x20)));
        assert_eq!(kb.state, AmigaKeyboardProtocolState::WaitHandshakeKey);
        assert_eq!(kb.queued_key_count(), 1);

        for _ in 0..HANDSHAKE_TIMEOUT_TICKS {
            assert_eq!(kb.tick(), None);
        }

        assert_eq!(kb.state, AmigaKeyboardProtocolState::Idle);
        assert_eq!(kb.queued_key_count(), 1);

        for _ in 0..BYTE_INTERVAL_TICKS - 1 {
            assert_eq!(kb.tick(), None);
        }
        assert_eq!(kb.tick(), Some(encode_keycode(0x21)));
    }
}
