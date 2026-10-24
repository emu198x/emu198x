//! Logical screen inspection must match bytes read by an actual 6502 guest.
use machine_nintendo_nes::Nes;
use nintendo_nes_cartridge::Mmc1;

fn write(program: &mut Vec<u8>, address: u16, value: u8) {
    program.extend_from_slice(&[0xA9, value, 0x8D, address as u8, (address >> 8) as u8]);
}

fn select_mode(program: &mut Vec<u8>, mode: u8) {
    for bit in 0..5 {
        write(program, 0x8000, ((0x0C | mode) >> bit) & 1);
    }
}

fn address(program: &mut Vec<u8>, address: u16) {
    program.extend_from_slice(&[0x2C, 0x02, 0x20]); // BIT PPUSTATUS resets toggle
    write(program, 0x2006, (address >> 8) as u8);
    write(program, 0x2006, address as u8);
}

fn guest_prg(modes: &[u8]) -> Vec<u8> {
    let mut program = vec![0x78, 0xD8, 0xA2, 0xFF, 0x9A]; // SEI/CLD/LDX/TXS
    write(&mut program, 0x2000, 0);
    write(&mut program, 0x2001, 0);
    write(&mut program, 0x4015, 0);
    write(&mut program, 0x00FE, 0);
    write(&mut program, 0x00FF, 0);
    program.extend_from_slice(&[0x2C, 0x02, 0x20]);
    program.extend_from_slice(&[0x2C, 0x02, 0x20, 0x10, 0xFB]);
    program.extend_from_slice(&[0x2C, 0x02, 0x20, 0x10, 0xFB]); // two fresh vblanks
    select_mode(&mut program, 2); // vertical: first 2 KiB reaches both CIRAM pages
    address(&mut program, 0x2000);
    for block in 0..8u8 {
        program.extend_from_slice(&[
            0xA2,
            0,    // LDX #0
            0x8A, // TXA
            0x49,
            block * 17, // EOR #seed: distinguish every 256-byte block
            0x8D,
            7,
            0x20, // STA PPUDATA
            0xE8, // INX
            0xD0,
            0xF7, // BNE TXA
        ]);
    }
    for (phase, mode) in modes.iter().enumerate() {
        select_mode(&mut program, *mode);
        address(&mut program, 0x2000);
        program.extend_from_slice(&[0xAD, 7, 0x20]); // discard old buffered byte
        for block in 0..8u8 {
            program.extend_from_slice(&[
                0xA2,
                0, // LDX #0
                0xAD,
                7,
                0x20, // LDA PPUDATA
                0x9D,
                0,
                0x60 + block, // STA $6000+block*256,X
                0xE8,         // INX
                0xD0,
                0xF7, // BNE LDA
            ]);
        }
        let marker = u8::try_from(phase + 1).expect("short mode sequence");
        write(&mut program, 0x00FF, marker);
        // Wait for the host to inspect this phase before changing the mapper.
        program.extend_from_slice(&[0xA5, 0xFE, 0xC9, marker, 0xD0, 0xFA]);
    }
    let stop = 0xC000 + u16::try_from(program.len()).expect("short guest");
    program.extend_from_slice(&[0x4C, stop as u8, (stop >> 8) as u8]);
    let mut prg = vec![0; 32768];
    prg[16384..16384 + program.len()].copy_from_slice(&program);
    prg[32764..32766].copy_from_slice(&[0, 0xC0]);
    prg
}

fn check_modes(modes: &[u8]) {
    let mut nes = Nes::new(Box::new(Mmc1::new(guest_prg(modes), vec![])));
    for (phase, mode) in modes.iter().enumerate() {
        let marker = u8::try_from(phase + 1).expect("short mode sequence");
        for _ in 0..8 {
            nes.run_frame();
            if nes.peek(0x00FF) == marker {
                break;
            }
        }
        assert_eq!(nes.peek(0x00FF), marker, "mode {mode}: guest must finish");
        // Mesen's SetMirroringType maps logical pages onto CIRAM as follows.
        let pages = match mode {
            0 => [0, 0], // lower single screen
            1 => [1, 1], // upper single screen
            2 => [0, 1], // vertical
            3 => [0, 0], // horizontal (first half of the logical 4 KiB space)
            _ => unreachable!(),
        };
        let guest: Vec<_> = (0..2048u16).map(|i| nes.peek(0x6000 + i)).collect();
        for (i, byte) in guest.iter().enumerate() {
            let physical = pages[i / 1024] * 1024 + i % 1024;
            let expected = physical as u8 ^ ((physical / 256) as u8 * 17);
            assert_eq!(*byte, expected, "guest mapping: mode {mode}, byte {i}");
        }
        let clock = nes.master_clock();
        let pc = nes.cpu.regs.pc;
        let ciram = *nes.ppu.nametable_ram();
        let visible = nes.effective_nametable();
        assert_eq!(nes.effective_nametable(), visible);
        assert_eq!(nes.master_clock(), clock);
        assert_eq!(nes.cpu.regs.pc, pc);
        assert_eq!(*nes.ppu.nametable_ram(), ciram);
        for (i, byte) in guest.iter().enumerate() {
            assert_eq!(visible[i], *byte, "inspection: mode {mode}, byte {i}");
        }
        nes.poke(0x00FE, marker);
    }
}

#[test]
fn horizontal_inspection_matches_guest() {
    check_modes(&[3]);
}

#[test]
fn vertical_inspection_matches_guest() {
    check_modes(&[2]);
}

#[test]
fn single_screen_lower_inspection_matches_guest() {
    check_modes(&[0]);
}

#[test]
fn single_screen_upper_inspection_matches_guest() {
    check_modes(&[1]);
}

#[test]
fn inspection_tracks_live_mirroring_changes() {
    check_modes(&[2, 3, 0, 1, 2]);
}
