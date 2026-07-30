//! Public diagnostic-snapshot coverage for the Amiga keyboard controller.

use peripheral_commodore_amiga_keyboard::{AmigaKeyboard, AmigaKeyboardProtocolState};

const POWERUP_DELAY_TICKS: u32 = 150_000;
const BYTE_INTERVAL_TICKS: u32 = 700;

fn encode(byte: u8) -> u8 {
    !byte.rotate_left(1)
}

fn advance_to_idle(keyboard: &mut AmigaKeyboard) {
    for _ in 0..POWERUP_DELAY_TICKS {
        assert_eq!(keyboard.tick(), None);
    }
    assert_eq!(keyboard.tick(), Some(encode(0xFD)));
    keyboard.handshake();
    assert_eq!(keyboard.tick(), Some(encode(0xFE)));
    keyboard.handshake();
}

#[test]
fn initial_snapshot_exposes_power_up_timer_and_model_granularity() {
    let keyboard = AmigaKeyboard::new();

    let snapshot = keyboard.diagnostic_snapshot();

    assert_eq!(snapshot.state, AmigaKeyboardProtocolState::PowerUpDelay);
    assert_eq!(snapshot.timer_ticks, 0);
    assert_eq!(snapshot.timer_limit_ticks, Some(POWERUP_DELAY_TICKS));
    assert_eq!(snapshot.timer_remaining_ticks, Some(POWERUP_DELAY_TICKS));
    assert!(!snapshot.timer_expired);
    assert_eq!(snapshot.current_byte, None);
    assert_eq!(snapshot.current_encoded_byte, None);
    assert_eq!(snapshot.pending_byte, Some(0xFD));
    assert_eq!(snapshot.pending_encoded_byte, Some(encode(0xFD)));
    assert_eq!(snapshot.serial_bit_index, None);
    assert_eq!(snapshot.serial_bits_completed, None);
    assert_eq!(snapshot.serial_bits_remaining, None);
    assert!(!snapshot.serial_transfer_active);
    assert!(snapshot.atomic_byte_delivery);
    assert!(snapshot.reset_sequence_active);
    assert!(!snapshot.reset_sequence_complete);
    assert!(!snapshot.waiting_for_handshake);
    assert!(!snapshot.waiting_for_byte_interval);
    assert!(!snapshot.timeout_will_resend);
    assert!(!snapshot.timeout_will_drop);
    assert!(!snapshot.transmission_ready);
    assert!(snapshot.queued_bytes.is_empty());
    assert_eq!(snapshot.queue_count, 0);
    assert!(snapshot.queue_allocated_capacity >= snapshot.queue_count);
    assert!(snapshot.queue_is_empty);
    assert_eq!(snapshot.bytes_sent, 0);
}

#[test]
fn power_up_snapshot_distinguishes_pending_and_delivered_bytes() {
    let mut keyboard = AmigaKeyboard::new();
    for _ in 0..POWERUP_DELAY_TICKS {
        assert_eq!(keyboard.tick(), None);
    }

    let pending = keyboard.diagnostic_snapshot();
    assert_eq!(pending.state, AmigaKeyboardProtocolState::SendInitPowerUp);
    assert_eq!(pending.timer_limit_ticks, None);
    assert_eq!(pending.pending_byte, Some(0xFD));
    assert_eq!(pending.pending_encoded_byte, Some(encode(0xFD)));
    assert_eq!(pending.current_byte, None);
    assert!(pending.transmission_ready);

    assert_eq!(keyboard.tick(), Some(encode(0xFD)));
    let delivered = keyboard.diagnostic_snapshot();
    assert_eq!(
        delivered.state,
        AmigaKeyboardProtocolState::WaitHandshakeInit
    );
    assert_eq!(delivered.current_byte, Some(0xFD));
    assert_eq!(delivered.current_encoded_byte, Some(encode(0xFD)));
    assert_eq!(delivered.pending_byte, Some(0xFE));
    assert_eq!(delivered.pending_encoded_byte, Some(encode(0xFE)));
    assert_eq!(delivered.serial_bits_completed, Some(8));
    assert_eq!(delivered.serial_bits_remaining, Some(0));
    assert!(delivered.waiting_for_handshake);
    assert!(delivered.timeout_will_resend);
    assert!(!delivered.timeout_will_drop);
    assert_eq!(delivered.bytes_sent, 1);

    keyboard.handshake();
    let terminate = keyboard.diagnostic_snapshot();
    assert_eq!(terminate.state, AmigaKeyboardProtocolState::SendTermPowerUp);
    assert_eq!(terminate.current_byte, None);
    assert_eq!(terminate.pending_byte, Some(0xFE));
}

#[test]
fn queued_key_snapshot_preserves_order_capacity_and_timeout_flags() {
    let mut keyboard = AmigaKeyboard::new();
    advance_to_idle(&mut keyboard);
    keyboard.key_event(0x45, true);
    keyboard.key_event(0x45, false);
    keyboard.key_event(0x20, true);

    let queued = keyboard.diagnostic_snapshot();
    assert_eq!(queued.state, AmigaKeyboardProtocolState::Idle);
    assert_eq!(queued.queued_bytes, vec![0x45, 0xC5, 0x20]);
    assert_eq!(queued.queue_count, 3);
    assert!(queued.queue_allocated_capacity >= queued.queue_count);
    assert!(!queued.queue_is_empty);
    assert_eq!(queued.pending_byte, Some(0x45));
    assert_eq!(queued.pending_encoded_byte, Some(encode(0x45)));
    assert!(queued.waiting_for_byte_interval);
    assert!(queued.reset_sequence_complete);

    for _ in 0..BYTE_INTERVAL_TICKS - 1 {
        assert_eq!(keyboard.tick(), None);
    }
    let almost_ready = keyboard.diagnostic_snapshot();
    assert_eq!(almost_ready.timer_remaining_ticks, Some(1));
    assert!(!almost_ready.transmission_ready);

    assert_eq!(keyboard.tick(), Some(encode(0x45)));
    let in_flight = keyboard.diagnostic_snapshot();
    assert_eq!(
        in_flight.state,
        AmigaKeyboardProtocolState::WaitHandshakeKey
    );
    assert_eq!(in_flight.current_byte, Some(0x45));
    assert_eq!(in_flight.queued_bytes, vec![0xC5, 0x20]);
    assert_eq!(in_flight.queue_count, 2);
    assert!(in_flight.waiting_for_handshake);
    assert!(!in_flight.timeout_will_resend);
    assert!(in_flight.timeout_will_drop);
    assert!(in_flight.reset_sequence_complete);
}

#[test]
fn taking_a_snapshot_does_not_advance_or_consume_protocol_state() {
    let mut keyboard = AmigaKeyboard::new();
    advance_to_idle(&mut keyboard);
    keyboard.key_event(0x20, true);
    for _ in 0..BYTE_INTERVAL_TICKS - 1 {
        assert_eq!(keyboard.tick(), None);
    }

    let state_before = keyboard.debug_state_name();
    let timer_before = keyboard.debug_timer();
    let queue_before = keyboard.queued_key_count();
    let sent_before = keyboard.bytes_sent;
    let first = keyboard.diagnostic_snapshot();
    let second = keyboard.diagnostic_snapshot();

    assert_eq!(first, second);
    assert_eq!(keyboard.debug_state_name(), state_before);
    assert_eq!(keyboard.debug_timer(), timer_before);
    assert_eq!(keyboard.queued_key_count(), queue_before);
    assert_eq!(keyboard.bytes_sent, sent_before);
    assert_eq!(keyboard.tick(), Some(encode(0x20)));
}
