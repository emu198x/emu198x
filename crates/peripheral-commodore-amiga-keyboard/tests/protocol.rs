//! Phase 1 characterisation — Amiga keyboard protocol.
//!
//! Covers task #173. Exercises `AmigaKeyboard` through its public
//! API only: `tick()`, `handshake()`, `key_event()`. Locks in
//! the encoding, power-up sequence, and handshake-timeout resend
//! behaviour before Phase 2 wires the keyboard into the machine.

use peripheral_commodore_amiga_keyboard::AmigaKeyboard;

/// Matches the archive's private `POWERUP_DELAY_TICKS` constant.
const POWERUP_DELAY_TICKS: u32 = 150_000;
/// Matches the archive's private `BYTE_INTERVAL_TICKS` constant.
const BYTE_INTERVAL_TICKS: u32 = 700;
/// Matches the archive's private `HANDSHAKE_TIMEOUT_TICKS` constant.
const HANDSHAKE_TIMEOUT_TICKS: u32 = 100_000;

/// The Amiga keyboard encoding: rotate left by 1 then invert.
/// KDAT is active-low on the wire, so the receiver sees `!encoded`;
/// the ROM decodes by inverting then rotating right.
fn encode(byte: u8) -> u8 {
    !byte.rotate_left(1)
}

fn advance_to_idle(kb: &mut AmigaKeyboard) {
    for _ in 0..POWERUP_DELAY_TICKS {
        kb.tick();
    }
    let init = kb.tick();
    assert_eq!(init, Some(encode(0xFD)));
    kb.handshake();
    let term = kb.tick();
    assert_eq!(term, Some(encode(0xFE)));
    kb.handshake();
}

#[test]
fn power_up_emits_init_then_terminate_sequence() {
    let mut kb = AmigaKeyboard::new();

    // Nothing emitted during the power-up delay window.
    for _ in 0..POWERUP_DELAY_TICKS - 1 {
        assert_eq!(kb.tick(), None);
    }
    // The tick that reaches the threshold transitions state but
    // produces no byte yet; the next tick sends $FD.
    assert_eq!(kb.tick(), None);
    assert_eq!(
        kb.tick(),
        Some(encode(0xFD)),
        "init-power-up byte \\$FD transmitted after delay"
    );

    // Host acknowledges; next tick sends $FE.
    kb.handshake();
    assert_eq!(
        kb.tick(),
        Some(encode(0xFE)),
        "terminate-power-up byte \\$FE transmitted after handshake"
    );
    kb.handshake();
}

#[test]
fn encode_round_trips_for_all_256_keycodes() {
    // ROM decodes rotated-inverted bytes by inverting + rotating right;
    // our encoder + that inverse must round-trip exactly.
    for byte in 0..=255u8 {
        let encoded = encode(byte);
        let recovered = (!encoded).rotate_right(1);
        assert_eq!(recovered, byte, "round-trip failed for ${byte:02X}");
    }
}

#[test]
fn encode_matches_winuae_formula() {
    // WinUAE `uae_kbd.c`: `kbcode = ~((keycode << 1) | (keycode >> 7))`
    // should match our `!byte.rotate_left(1)` exactly for every keycode.
    for byte in 0..=255u8 {
        let winuae = !byte.rotate_left(1);
        assert_eq!(encode(byte), winuae, "mismatch at ${byte:02X}");
    }
}

#[test]
fn init_byte_is_resent_after_handshake_timeout() {
    let mut kb = AmigaKeyboard::new();
    for _ in 0..POWERUP_DELAY_TICKS {
        kb.tick();
    }
    // First $FD goes out, bytes_sent = 1.
    assert_eq!(kb.tick(), Some(encode(0xFD)));
    assert_eq!(kb.bytes_sent, 1);

    // No handshake — wait out the timeout.
    for _ in 0..HANDSHAKE_TIMEOUT_TICKS {
        assert_eq!(kb.tick(), None);
    }
    // Second $FD is emitted on the next tick.
    assert_eq!(
        kb.tick(),
        Some(encode(0xFD)),
        "keyboard retransmits \\$FD after handshake timeout"
    );
    assert_eq!(kb.bytes_sent, 2);
}

#[test]
fn terminate_byte_is_resent_after_handshake_timeout() {
    let mut kb = AmigaKeyboard::new();
    for _ in 0..POWERUP_DELAY_TICKS {
        kb.tick();
    }
    assert_eq!(kb.tick(), Some(encode(0xFD)));
    kb.handshake();
    assert_eq!(kb.tick(), Some(encode(0xFE)));
    // Hold the handshake.
    for _ in 0..HANDSHAKE_TIMEOUT_TICKS {
        assert_eq!(kb.tick(), None);
    }
    assert_eq!(
        kb.tick(),
        Some(encode(0xFE)),
        "keyboard retransmits \\$FE after handshake timeout"
    );
    assert_eq!(kb.bytes_sent, 3);
}

#[test]
fn key_events_queue_in_fifo_order_with_press_release_bit_7() {
    let mut kb = AmigaKeyboard::new();
    advance_to_idle(&mut kb);

    kb.key_event(0x45, true); // Enter press
    kb.key_event(0x45, false); // Enter release
    kb.key_event(0x20, true); // Space press
    assert_eq!(kb.queued_key_count(), 3);

    // Key press: bit 7 clear.
    for _ in 0..BYTE_INTERVAL_TICKS - 1 {
        assert_eq!(kb.tick(), None);
    }
    assert_eq!(kb.tick(), Some(encode(0x45)), "Enter down -> raw $45");
    kb.handshake();

    for _ in 0..BYTE_INTERVAL_TICKS - 1 {
        assert_eq!(kb.tick(), None);
    }
    assert_eq!(
        kb.tick(),
        Some(encode(0xC5)),
        "Enter up -> raw $C5 (bit 7 set)"
    );
    kb.handshake();

    for _ in 0..BYTE_INTERVAL_TICKS - 1 {
        assert_eq!(kb.tick(), None);
    }
    assert_eq!(kb.tick(), Some(encode(0x20)), "Space down -> raw $20");
    kb.handshake();

    assert_eq!(kb.queued_key_count(), 0);
}

#[test]
fn key_byte_handshake_timeout_drops_in_flight_byte_but_keeps_queue() {
    let mut kb = AmigaKeyboard::new();
    advance_to_idle(&mut kb);

    kb.key_event(0x20, true);
    kb.key_event(0x21, true);

    for _ in 0..BYTE_INTERVAL_TICKS - 1 {
        assert_eq!(kb.tick(), None);
    }
    assert_eq!(kb.tick(), Some(encode(0x20)), "first queued key goes out");
    assert_eq!(
        kb.queued_key_count(),
        1,
        "second key remains queued while we wait for handshake"
    );

    // No handshake — timeout drops the byte.
    for _ in 0..HANDSHAKE_TIMEOUT_TICKS {
        assert_eq!(kb.tick(), None);
    }
    // Next byte-interval transmits the SECOND queued key; the first
    // is considered lost (matches WinUAE).
    for _ in 0..BYTE_INTERVAL_TICKS - 1 {
        assert_eq!(kb.tick(), None);
    }
    assert_eq!(
        kb.tick(),
        Some(encode(0x21)),
        "second queued key is sent after timeout drops the first"
    );
}
