//! Original guests read trainers through the CPU bus and then modify their RAM.
use machine_nintendo_nes::Nes;
use nintendo_nes_cartridge::parse_ines;

fn write(program: &mut Vec<u8>, address: u16, value: u8) {
    program.extend_from_slice(&[0xA9, value, 0x8D, address as u8, (address >> 8) as u8]);
}

fn image(mapper: u8, nes2: bool, battery: bool, trainer: bool) -> Vec<u8> {
    let mut header = vec![0; 16];
    header[..4].copy_from_slice(b"NES\x1a");
    header[4] = 2;
    header[5] = 1;
    header[6] = (mapper << 4) | (u8::from(battery) * 2) | (u8::from(trainer) * 4);
    header[7] = (mapper & 0xF0) | if nes2 { 8 } else { 0 };
    if nes2 {
        header[10] = if battery { 0x70 } else { 7 };
    }
    let mut program = vec![0x78, 0xD8, 0xA2, 0xFF, 0x9A]; // SEI/CLD/LDX/TXS
    write(&mut program, 0x00FF, 0);
    write(&mut program, 0x2000, 0);
    write(&mut program, 0x2001, 0);
    write(&mut program, 0x4015, 0);
    match mapper {
        4 => write(&mut program, 0xA001, 0x80), // enable ordinary PRG RAM
        5 => {
            write(&mut program, 0x5102, 2);
            write(&mut program, 0x5103, 1); // enable guest RAM writes
        }
        68 => write(&mut program, 0xF000, 0x10),
        _ => {}
    }
    for page in 0..2u8 {
        program.extend_from_slice(&[
            0xA2,
            0, // LDX #0
            0xBD,
            0,
            0x70 + page, // LDA $7000+page*256,X
            0x9D,
            0,
            4 + page, // STA $0400+page*256,X
            0xE8,     // INX
            0xD0,
            0xF7, // BNE LDA
        ]);
    }
    write(&mut program, 0x7000, 0x5A); // a trainer is writable RAM after loading
    write(&mut program, 0x71FF, 0xA5);
    write(&mut program, 0x00FF, 0xA5);
    let stop = 0xE000 + u16::try_from(program.len()).expect("short guest");
    program.extend_from_slice(&[0x4C, stop as u8, (stop >> 8) as u8]);
    let mut prg = vec![0; 32768];
    prg[24576..24576 + program.len()].copy_from_slice(&program);
    prg[32764..32766].copy_from_slice(&[0, 0xE0]);
    if trainer {
        header.extend((0..512).map(|i| (i as u8).wrapping_mul(37).wrapping_add(11)));
    }
    header.extend(prg);
    header.extend(vec![0; 8192]);
    header
}

#[test]
fn guests_read_modify_and_restore_loaded_trainers() {
    for mapper in [0, 1, 3, 4, 5, 34, 68] {
        for nes2 in [false, true] {
            for battery in [false, true] {
                for trainer in [false, true] {
                    let cart =
                        parse_ines(&image(mapper, nes2, battery, trainer)).expect("valid guest");
                    let mut nes = Nes::new(cart.mapper);
                    for _ in 0..4 {
                        nes.run_frame();
                        if nes.peek(0x00FF) == 0xA5 {
                            break;
                        }
                    }
                    assert_eq!(nes.peek(0x00FF), 0xA5, "mapper {mapper}: guest must finish");
                    for i in 0..512u16 {
                        let expected = if trainer {
                            (i as u8).wrapping_mul(37).wrapping_add(11)
                        } else {
                            0
                        };
                        assert_eq!(
                            nes.peek(0x0400 + i),
                            expected,
                            "mapper {mapper}, NES2 {nes2}, battery {battery}, trainer {trainer}, byte {i}"
                        );
                    }
                    assert_eq!(nes.peek(0x7000), 0x5A);
                    assert_eq!(nes.peek(0x71FF), 0xA5);
                    let mut restored = Nes::from_snapshot(nes.snapshot());
                    restored.run_frame();
                    assert_eq!(
                        restored.peek(0x7000),
                        0x5A,
                        "restore must not reapply trainer"
                    );
                    assert_eq!(restored.peek(0x71FF), 0xA5);
                    assert_eq!(restored.mapper.save_ram(), nes.mapper.save_ram());
                }
            }
        }
    }
}
