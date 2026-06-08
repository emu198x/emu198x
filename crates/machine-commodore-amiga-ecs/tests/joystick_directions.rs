//! Regression: Amiga joystick direction → JOY1DAT cross-wiring (ECS).
//!
//! The joystick is cross-wired into the two pot pairs (verified vs
//! vAmiga Joystick::joydat() + HRM Appendix A): the X pair (JOY1DAT
//! bits 1,0) carries RIGHT + DOWN, the Y pair (bits 9,8) carries LEFT
//! + UP. A prior bug used the naive X=horizontal / Y=vertical layout,
//! so LEFT read as DOWN and DOWN read as LEFT in real games.

use machine_commodore_amiga_ecs::AmigaEcs;

const JOY1DAT: u32 = 0x00DF_F00C;
const PAIRS: u16 = 0x0303; // both pot pairs' low two bits

fn press(amiga: &mut AmigaEcs, dir: &str) -> u16 {
    assert!(amiga.set_joystick_control(1, dir, true));
    let v = amiga.read_word(JOY1DAT) & PAIRS;
    assert!(amiga.set_joystick_control(1, dir, false));
    v
}

#[test]
fn joystick_directions_match_hardware_cross_wiring() {
    let mut a = AmigaEcs::new(vec![0u8; 256 * 1024]);
    assert_eq!(press(&mut a, "right"), 0x0003, "RIGHT → X pair (bits 1,0)");
    assert_eq!(press(&mut a, "left"), 0x0300, "LEFT → Y pair (bits 9,8)");
    assert_eq!(press(&mut a, "up"), 0x0100, "UP → bit 8 (Y0 xor Y1)");
    assert_eq!(press(&mut a, "down"), 0x0001, "DOWN → bit 0 (X0 xor X1)");
}
