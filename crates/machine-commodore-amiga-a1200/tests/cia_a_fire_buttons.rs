//! Regression: CIA-A PRA fire-button bit mapping (A1200).
//!
//! Real hardware: PA6 = /FIR0 (controller port 0 — the mouse left
//! button), PA7 = /FIR1 (controller port 1), both active-low. A prior
//! bug swapped the two bits across all three machine variants, so the
//! port-0 mouse-left button landed on PA7 instead of PA6 (surfaced by
//! Code198x Unit 10 authoring).

use machine_commodore_amiga_a1200::AmigaA1200;

const PRA: u32 = 0x00BF_E001;
const FIR0: u16 = 1 << 6; // controller port 0 fire (mouse left button)
const FIR1: u16 = 1 << 7; // controller port 1 fire

fn machine() -> AmigaA1200 {
    // The fire-bit mapping is pure CIA-A I/O logic, independent of ROM
    // contents; a blank power-of-two image is enough to construct.
    AmigaA1200::new(vec![0u8; 256 * 1024])
}

#[test]
fn port0_mouse_left_pulls_fir0_on_pa6() {
    let mut amiga = machine();

    // No buttons: both fire lines float high (active-low).
    let base = amiga.read_word(PRA);
    assert_eq!(base & FIR0, FIR0, "FIR0 (PA6) high with no button");
    assert_eq!(base & FIR1, FIR1, "FIR1 (PA7) high with no button");

    // Port-0 mouse left button: /FIR0 (PA6) goes low, PA7 untouched.
    amiga.set_mouse_button_port0("left", true);
    let pra = amiga.read_word(PRA);
    assert_eq!(pra & FIR0, 0, "mouse-left pulls /FIR0 (PA6) low");
    assert_eq!(pra & FIR1, FIR1, "port-1 /FIR1 (PA7) stays high");

    amiga.set_mouse_button_port0("left", false);
    assert_eq!(amiga.read_word(PRA) & FIR0, FIR0, "release restores PA6");
}

#[test]
fn port1_joystick_fire_pulls_fir1_on_pa7() {
    let mut amiga = machine();

    // Port-1 joystick fire: /FIR1 (PA7) goes low, PA6 untouched.
    assert!(amiga.set_joystick_control(1, "fire", true));
    let pra = amiga.read_word(PRA);
    assert_eq!(pra & FIR1, 0, "joy-1 fire pulls /FIR1 (PA7) low");
    assert_eq!(pra & FIR0, FIR0, "port-0 /FIR0 (PA6) stays high");
}
