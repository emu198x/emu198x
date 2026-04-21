//! Phase 3 machine-level integration tests for the Amiga keyboard.
//!
//! Closes task #175 — the final keyboard port milestone. Drives the
//! full machine tick loop and confirms the keyboard state machine
//! delivers the power-up sequence + queued key events through CIA-A
//! SDR, with CRA bit 6 (SPMODE) rising edges acting as the host
//! handshake.

use machine_commodore_amiga_ocs::AmigaOcs;

fn zero_rom() -> Vec<u8> {
    vec![0; 512 * 1024]
}

/// Synthetic host "handshake": pulse CIA-A CRA bit 6 (SPMODE) 0→1→0
/// on top of whatever the CPU has set. We reach through
/// `cia_a_mut()` because the test doesn't run real ROM — the CPU is
/// executing zero-ROM instructions and will never touch CRA.
fn pulse_handshake(amiga: &mut AmigaOcs) {
    // Snapshot current CRA, set bit 6, tick once at E-clock rate so
    // the machine observes the rising edge, then clear bit 6 and
    // tick again so we can detect the next rise.
    let cra = amiga.cia_a().cra();
    amiga.cia_a_mut().write(0x0E, cra | 0x40);
    advance_eclock(amiga, 1);
    amiga.cia_a_mut().write(0x0E, cra & !0x40);
    advance_eclock(amiga, 1);
}

/// Advance the machine by `n` E-clock ticks' worth of CPU cycles.
/// One E-clock = 10 master/4 ticks = 5 CCKs = 10 `amiga.tick()`s.
fn advance_eclock(amiga: &mut AmigaOcs, n: u32) {
    for _ in 0..n {
        for _ in 0..10 {
            amiga.tick();
        }
    }
}

/// Encode matches the keyboard's rotate+invert protocol.
fn encode(byte: u8) -> u8 {
    !byte.rotate_left(1)
}

#[test]
fn fresh_machine_has_keyboard_in_power_up_delay() {
    let amiga = AmigaOcs::new(zero_rom());
    assert_eq!(amiga.keyboard().debug_state_name(), "PowerUpDelay");
    assert_eq!(amiga.keyboard().bytes_sent, 0);
}

#[test]
fn keyboard_emits_fd_byte_into_cia_sdr_after_power_up_delay() {
    let mut amiga = AmigaOcs::new(zero_rom());
    // Power-up delay is 150_000 E-clock ticks. Advance a bit past it
    // and confirm the init byte has landed in SDR.
    advance_eclock(&mut amiga, 150_001);
    assert_eq!(amiga.cia_a().sdr(), encode(0xFD),
        "CIA-A SDR should hold rotated-inverted \\$FD");
    assert_eq!(amiga.keyboard().bytes_sent, 1);
}

#[test]
fn handshake_advances_state_machine_through_fd_then_fe_to_idle() {
    let mut amiga = AmigaOcs::new(zero_rom());
    advance_eclock(&mut amiga, 150_001);
    assert_eq!(amiga.cia_a().sdr(), encode(0xFD));

    // Synthetic host handshake — triggers transition to $FE send.
    pulse_handshake(&mut amiga);
    // Give the state machine one tick to emit the next byte.
    advance_eclock(&mut amiga, 1);
    assert_eq!(amiga.cia_a().sdr(), encode(0xFE),
        "second byte after first handshake should be \\$FE");

    pulse_handshake(&mut amiga);
    advance_eclock(&mut amiga, 1);
    assert_eq!(amiga.keyboard().debug_state_name(), "Idle",
        "second handshake lands the controller in Idle");
    assert_eq!(amiga.keyboard().bytes_sent, 2);
}

#[test]
fn queued_key_event_reaches_cia_sdr_after_power_up() {
    let mut amiga = AmigaOcs::new(zero_rom());
    // Power-up sequence.
    advance_eclock(&mut amiga, 150_001);
    pulse_handshake(&mut amiga);
    advance_eclock(&mut amiga, 1);
    pulse_handshake(&mut amiga);
    advance_eclock(&mut amiga, 1);

    // Queue a key press ($45 = Enter).
    amiga.key_event(0x45, true);
    assert_eq!(amiga.keyboard().queued_key_count(), 1);

    // Byte interval is 700 E-clock ticks.
    advance_eclock(&mut amiga, 701);
    assert_eq!(amiga.cia_a().sdr(), encode(0x45),
        "queued key press arrives rotated-inverted at SDR");
    assert_eq!(amiga.keyboard().queued_key_count(), 0);
}

