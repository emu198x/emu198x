//! Screen inspection must describe the ROM bytes a guest reads through the PPU.
use common_nintendo_nes::Nes;
use nintendo_nes_cartridge::{Mirroring, Sunsoft4};

const ADDRESSES: [u16; 8] = [
    0x2000, 0x2001, 0x23C0, 0x23FF, 0x2400, 0x2401, 0x27C0, 0x27FF,
];

fn write_register(program: &mut Vec<u8>, address: u16, value: u8) {
    program.extend_from_slice(&[0xA9, value, 0x8D, address as u8, (address >> 8) as u8]);
}

fn machine(mode: u8) -> Nes {
    let mut program = vec![
        0x78, // SEI
        0xD8, // CLD
        0xA2, 0xFF, 0x9A, // LDX #$FF / TXS
    ];
    write_register(&mut program, 0x2000, 0);
    write_register(&mut program, 0x2001, 0);
    write_register(&mut program, 0x4015, 0);
    // Clear any pending VBLANK, then wait for two fresh ones so PPU writes work.
    program.extend_from_slice(&[
        0x2C, 0x02, 0x20, // BIT $2002
        0x2C, 0x02, 0x20, 0x10, 0xFB, // BIT $2002 / BPL back
        0x2C, 0x02, 0x20, 0x10, 0xFB, // BIT $2002 / BPL back
    ]);
    write_register(&mut program, 0xC000, 5);
    write_register(&mut program, 0xD000, 0x36);
    write_register(&mut program, 0xE000, 0x10 | mode);
    for (index, address) in ADDRESSES.iter().enumerate() {
        program.extend_from_slice(&[0x2C, 0x02, 0x20]); // reset address toggle
        write_register(&mut program, 0x2006, (address >> 8) as u8);
        write_register(&mut program, 0x2006, *address as u8);
        program.extend_from_slice(&[
            0xAD,
            0x07,
            0x20, // LDA $2007: discard previous buffered byte
            0xAD,
            0x07,
            0x20, // LDA $2007: byte at requested address
            0x85,
            index as u8, // STA zero-page result
        ]);
    }
    write_register(&mut program, 0x00FF, 0xA5);
    let stop = 0xC000 + u16::try_from(program.len()).expect("short guest program");
    program.extend_from_slice(&[0x4C, stop as u8, (stop >> 8) as u8]);

    let mut prg = vec![0; 128 * 1024];
    let last_bank = prg.len() - 16384;
    prg[last_bank..last_bank + program.len()].copy_from_slice(&program);
    let vector = prg.len() - 4;
    prg[vector..vector + 2].copy_from_slice(&[0x00, 0xC0]);
    let chr = (0..256 * 1024)
        .map(|offset| (offset / 1024) as u8 ^ offset as u8)
        .collect();
    Nes::new(Box::new(Sunsoft4::new(prg, chr, Mirroring::Vertical)))
}

#[test]
fn sunsoft4_screen_inspection_matches_guest_ppudata_reads() {
    for mode in 0..4 {
        let mut nes = machine(mode);
        for _ in 0..6 {
            nes.run_frame();
            if nes.peek(0x00FF) == 0xA5 {
                break;
            }
        }
        assert_eq!(nes.peek(0x00FF), 0xA5, "mode {mode}: guest must finish");
        // The guest has not written CIRAM: it reads ROM nametables instead.
        assert!(nes.ppu.nametable_ram().iter().all(|byte| *byte == 0));
        let visible = nes.effective_nametable();
        for (index, address) in ADDRESSES.iter().enumerate() {
            let fetched = nes.peek(index as u16);
            assert_ne!(fetched, 0, "fixture must distinguish ROM from empty CIRAM");
            assert_eq!(
                visible[usize::from(address - 0x2000)],
                fetched,
                "mode {mode}, PPU address {address:04X}"
            );
        }
    }
}
