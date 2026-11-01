//! Home-console controller wiring, not the SC-3000 PPI or arcade board.
//! Sources: shared reference/by-system/sega-sg-1000/2026-controller-wiring-observations.md;
//! Ares ares/sg/cpu/memory.cpp and MAME sg1000_state::peripheral_r.
use common_sega_sg_1000::{ControllerState, Sg1000, Sg1000Region};
use emu198x_z80_cadence::Z80Machine;

// Bit masks from the home-console wiring table, in host input order:
// P1 U/D/L/R/1/2 followed by P2 U/D/L/R/1/2.
const INPUT_BITS: [(u8, u8); 12] = [
    (0x01, 0),
    (0x02, 0),
    (0x04, 0),
    (0x08, 0),
    (0x10, 0),
    (0x20, 0),
    (0x40, 0),
    (0x80, 0),
    (0, 0x01),
    (0, 0x02),
    (0, 0x04),
    (0, 0x08),
];

fn controller(bits: u16) -> ControllerState {
    ControllerState {
        up: bits & 1 != 0,
        down: bits & 2 != 0,
        left: bits & 4 != 0,
        right: bits & 8 != 0,
        button1: bits & 16 != 0,
        button2: bits & 32 != 0,
    }
}

fn expected(state: u16) -> [u8; 2] {
    let mut result = [0xFF, 0xFF];
    for (index, &(even, odd)) in INPUT_BITS.iter().enumerate() {
        if state & (1 << index) != 0 {
            result[0] &= !even;
            result[1] &= !odd;
        }
    }
    result
}

fn cartridge() -> Vec<u8> {
    let mut code = vec![0xF3]; // DI
    // Read each alias through real Z80 IN instructions and store into RAM.
    for port in 0xC0..=0xFFu8 {
        code.extend([0xDB, port, 0x32, port - 0xC0, 0xC0]);
    }
    code.extend([0x3E, 0xA5, 0x32, 0x40, 0xC0, 0xC3, 0x01, 0x00]);
    code.resize(0x8000, 0);
    code
}

fn set_input(machine: &mut Sg1000, state: u16) {
    *machine.controller1_mut() = controller(state);
    *machine.controller2_mut() = controller(state >> 6);
}

fn capture(machine: &mut Sg1000) -> [u8; 64] {
    machine.poke(0xC040, 0);
    machine.advance_tstates(4_000); // At least two complete guest scans.
    assert_eq!(machine.peek(0xC040), 0xA5, "guest must finish a scan");
    std::array::from_fn(|i| machine.peek(0xC000 + u16::try_from(i).expect("small offset")))
}

#[test]
fn all_controller_states_reach_the_wired_bits_in_every_alias() {
    let rom = cartridge();
    let mut failed_states = 0;
    let mut first_failures = Vec::new();
    let mut passed_states = 0;
    for region in [Sg1000Region::Ntsc, Sg1000Region::Pal] {
        let mut machine = Sg1000::new(rom.clone(), region);
        for state in 0..4096 {
            set_input(&mut machine, state);
            let want = expected(state);
            let actual = capture(&mut machine);
            if actual
                .iter()
                .enumerate()
                .all(|(i, &byte)| byte == want[i & 1])
            {
                passed_states += 1;
            } else {
                failed_states += 1;
                if first_failures.len() < 8 {
                    first_failures.push((region, state, [actual[0], actual[1]], want));
                }
            }
        }
    }
    assert_eq!(passed_states + failed_states, 8192);
    assert_eq!(
        failed_states, 0,
        "{passed_states} controls passed; first faults: {first_failures:?}"
    );
}

#[test]
fn both_controllers_survive_snapshot_and_release_without_stuck_bits() {
    for region in [Sg1000Region::Ntsc, Sg1000Region::Pal] {
        let mut original = Sg1000::new(cartridge(), region);
        set_input(&mut original, 0x555);
        capture(&mut original);
        let bytes = postcard::to_allocvec(&original).expect("snapshot");
        let mut restored: Sg1000 = postcard::from_bytes(&bytes).expect("restore");
        restored.cpu_mut().rehydrate_walker_sequence();
        for state in [0x555, 0, 0xAAA, 0xFFF, 0] {
            set_input(&mut original, state);
            set_input(&mut restored, state);
            let actual = capture(&mut original);
            assert_eq!(capture(&mut restored), actual);
            for (index, byte) in actual.into_iter().enumerate() {
                assert_eq!(
                    byte,
                    expected(state)[index & 1],
                    "{region:?}, state {state:03x}, alias {index:02x}"
                );
            }
        }
    }
}
