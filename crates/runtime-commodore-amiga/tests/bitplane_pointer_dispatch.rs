//! HRM register map: BPL6PTL is $0F6. AGA extends the pairs through $0FE;
//! WinUAE custom.cpp dispatches BPLxPTH/BPLxPTL for all eight planes.
use common_commodore_amiga::AmigaDriver;
use machine_commodore_amiga_a1200::AmigaA1200;
use machine_commodore_amiga_ecs::AmigaEcs;
use machine_commodore_amiga_ocs::AmigaOcs;

const POINTERS: [u32; 8] = [
    0x012340, 0x023450, 0x034560, 0x045670, 0x056780, 0x067890, 0x0789A0, 0x089AB0,
];

fn rom(cpu_writes: bool) -> Vec<u8> {
    let mut rom = vec![0; 512 * 1024];
    rom[..4].copy_from_slice(&0x4000u32.to_be_bytes());
    rom[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes());
    let mut code = Vec::new();
    if cpu_writes {
        for (plane, pointer) in POINTERS.iter().enumerate() {
            for (half, value) in [(*pointer >> 16) as u16, *pointer as u16 | 1]
                .into_iter()
                .enumerate()
            {
                // MOVE.W #value,$DFF0E0+plane*4+half*2 through real CPU pins.
                code.extend_from_slice(&0x33FCu16.to_be_bytes());
                code.extend_from_slice(&value.to_be_bytes());
                code.extend_from_slice(
                    &(0x00DF_F0E0u32 + plane as u32 * 4 + half as u32 * 2).to_be_bytes(),
                );
            }
        }
    }
    code.extend_from_slice(&0x60FEu16.to_be_bytes()); // BRA.S self
    rom[8..8 + code.len()].copy_from_slice(&code);
    rom
}

macro_rules! pointer_dispatch {
    ($name:ident, $machine:ty, $planes:expr, $copper:expr) => {
        #[test]
        fn $name() {
            let mut machine = <$machine>::new(rom(!$copper));
            if $copper {
                let mut address = 0x1000;
                for (plane, pointer) in POINTERS.iter().enumerate() {
                    for (half, value) in [(*pointer >> 16) as u16, *pointer as u16 | 1]
                        .into_iter()
                        .enumerate()
                    {
                        machine
                            .memory_mut()
                            .write_word(address, 0xE0 + plane as u16 * 4 + half as u16 * 2);
                        machine.memory_mut().write_word(address + 2, value);
                        address += 4;
                    }
                }
                machine.memory_mut().write_word(address, 0xFFFF);
                machine.memory_mut().write_word(address + 2, 0xFFFE);
                machine.poke_word(0xDFF080, 0);
                machine.poke_word(0xDFF082, 0x1000);
                machine.poke_word(0xDFF096, 0x8280);
                machine.poke_word(0xDFF088, 0);
            }
            for _ in 0..20_000 {
                machine.tick();
            }
            for (plane, pointer) in POINTERS.iter().enumerate() {
                let expected = if plane < $planes { *pointer } else { 0 };
                assert_eq!(
                    machine.agnus().bpl_pt[plane],
                    expected,
                    "plane {} pointer through {}",
                    plane + 1,
                    if $copper { "Copper" } else { "CPU" }
                );
            }
        }
    };
}

pointer_dispatch!(ocs_cpu_all_six_pointer_pairs, AmigaOcs, 6, false);
pointer_dispatch!(ecs_cpu_all_six_pointer_pairs, AmigaEcs, 6, false);
pointer_dispatch!(aga_cpu_all_eight_pointer_pairs, AmigaA1200, 8, false);
pointer_dispatch!(ocs_copper_all_six_pointer_pairs, AmigaOcs, 6, true);
pointer_dispatch!(ecs_copper_all_six_pointer_pairs, AmigaEcs, 6, true);
pointer_dispatch!(aga_copper_all_eight_pointer_pairs, AmigaA1200, 8, true);
