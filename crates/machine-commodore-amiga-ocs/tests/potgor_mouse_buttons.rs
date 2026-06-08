//! Regression: port-0 mouse right/middle buttons reach the correct
//! POTGOR bits end-to-end (machine → Paula). The RIGHT button is bit 10
//! (Intuition's menu button); MIDDLE is bit 8. They were previously
//! swapped on both ports — verified against vAmiga (Mouse::changePotgo)
//! and WinUAE. The mapping is pure I/O logic, so a blank power-of-two
//! ROM is enough to construct.

use machine_commodore_amiga_ocs::AmigaOcs;

const POTGOR: u32 = 0x00DF_F016;
const PORT0_MIDDLE: u16 = 1 << 8;
const PORT0_RIGHT: u16 = 1 << 10;

fn machine() -> AmigaOcs {
    AmigaOcs::new(vec![0u8; 256 * 1024])
}

#[test]
fn port0_right_button_pulls_potgor_bit10() {
    let mut amiga = machine();

    let base = amiga.read_word(POTGOR);
    assert_eq!(base & PORT0_RIGHT, PORT0_RIGHT, "right idle high");
    assert_eq!(base & PORT0_MIDDLE, PORT0_MIDDLE, "middle idle high");

    amiga.set_mouse_button_port0("right", true);
    let v = amiga.read_word(POTGOR);
    assert_eq!(v & PORT0_RIGHT, 0, "right button pulls POTGOR bit 10 low");
    assert_eq!(v & PORT0_MIDDLE, PORT0_MIDDLE, "middle (bit 8) unaffected");
}

#[test]
fn port0_middle_button_pulls_potgor_bit8() {
    let mut amiga = machine();

    amiga.set_mouse_button_port0("middle", true);
    let v = amiga.read_word(POTGOR);
    assert_eq!(v & PORT0_MIDDLE, 0, "middle button pulls POTGOR bit 8 low");
    assert_eq!(v & PORT0_RIGHT, PORT0_RIGHT, "right (bit 10) unaffected");
}

#[test]
fn joystick_port1_second_and_third_buttons_reach_potgor() {
    // A two-button / CD32-style pad's 2nd and 3rd fire buttons read on
    // port 1's POTGOR pot lines (the same pins the mouse right / middle
    // use): button2 → bit 14, button3 → bit 12. Verified vs vAmiga.
    let mut amiga = machine();
    assert_eq!(
        amiga.read_word(POTGOR) & 0x5000,
        0x5000,
        "both pins idle high"
    );

    assert!(amiga.set_joystick_control(1, "button2", true));
    let v = amiga.read_word(POTGOR);
    assert_eq!(v & (1 << 14), 0, "2nd fire pulls POTGOR bit 14 low");
    assert_eq!(v & (1 << 12), 1 << 12, "3rd-button pin untouched");

    assert!(amiga.set_joystick_control(1, "button3", true));
    assert_eq!(
        amiga.read_word(POTGOR) & (1 << 12),
        0,
        "3rd fire pulls bit 12 low"
    );
}
