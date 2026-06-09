//! Regression: JOY1DAT digital-joystick decode + clean neutral return.
//!
//! Code198x Blitz-primer units 9 + 11 read `Joyx(1)` / `Joyy(1)`, which
//! decode JOY1DAT (`$DFF00C`) as: right = bit1, left = bit9,
//! down = bit0 ^ bit1, up = bit8 ^ bit9. A neutral stick must therefore
//! read JOY1DAT = 0, or the decode yields a phantom direction and the
//! sprite drifts (issue #120 — "−1/−1 at neutral").
//!
//! These assertions pin the read path the new `amiga.input.joy1dat`
//! query exposes: neutral is exactly zero, each direction sets exactly
//! its decoded bits, and releasing returns cleanly to zero with no
//! stuck bits.

use machine_commodore_amiga_ocs::AmigaOcs;

fn machine() -> AmigaOcs {
    AmigaOcs::new(vec![0u8; 256 * 1024])
}

#[test]
fn neutral_joystick_reads_joy1dat_zero() {
    let amiga = machine();
    assert_eq!(amiga.joy1dat(), 0, "a centred stick must read JOY1DAT = 0");
}

#[test]
fn directions_decode_to_expected_joy1dat_bits() {
    // right → bit1 (x = 0b11): right=1, down=0 → (1<<1)|(1^0) = 0b11.
    let mut amiga = machine();
    assert!(amiga.set_joystick_control(1, "right", true));
    assert_eq!(amiga.joy1dat(), 0x0003, "right = JOY1DAT bits 0+1");
    assert!(amiga.set_joystick_control(1, "right", false));
    assert_eq!(amiga.joy1dat(), 0, "release right returns to neutral");

    // up → bit8 (y = 0b01): left=0, up=1 → (0<<1)|(0^1) = 0b01.
    assert!(amiga.set_joystick_control(1, "up", true));
    assert_eq!(amiga.joy1dat(), 0x0100, "up = JOY1DAT bit 8");
    assert!(amiga.set_joystick_control(1, "up", false));
    assert_eq!(amiga.joy1dat(), 0, "release up returns to neutral");
}

#[test]
fn press_then_release_leaves_no_stuck_bits() {
    // Cycle every direction and confirm the register settles to a clean
    // zero each time — the intermittent-drift failure mode from #120.
    let mut amiga = machine();
    for name in ["up", "down", "left", "right"] {
        assert!(amiga.set_joystick_control(1, name, true));
        assert_ne!(amiga.joy1dat(), 0, "{name} press must move JOY1DAT");
        assert!(amiga.set_joystick_control(1, name, false));
        assert_eq!(amiga.joy1dat(), 0, "{name} release must clear JOY1DAT");
    }
}
