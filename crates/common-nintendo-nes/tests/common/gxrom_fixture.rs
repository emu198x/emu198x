// Original CPU guest shared by the regression and reference probe.
fn write(program: &mut Vec<u8>, address: u16, value: u8) {
    program.extend_from_slice(&[0xA9, value, 0x8D, address as u8, (address >> 8) as u8]);
}
fn observe(program: &mut Vec<u8>, slot: u16) {
    program.extend_from_slice(&[0xAD, 0, 0x90, 0x8D, slot as u8, (slot >> 8) as u8]);
    program.extend_from_slice(&[0x2C, 2, 0x20]); // reset PPU address toggle
    write(program, 0x2006, 0);
    write(program, 0x2006, 0);
    program.extend_from_slice(&[0xAD, 7, 0x20, 0xAD, 7, 0x20]); // delayed CHR read
    let destination = slot + 0x40;
    program.extend_from_slice(&[0x8D, destination as u8, (destination >> 8) as u8]);
}
pub fn cartridge() -> Vec<u8> {
    let mut program = vec![0x78, 0xD8, 0xA2, 0xFF, 0x9A];
    write(&mut program, 0x2000, 0);
    write(&mut program, 0x2001, 0);
    // Let the PPU complete its reset interval before writing PPUADDR.
    program.extend_from_slice(&[
        0x2C, 2, 0x20, 0x2C, 2, 0x20, 0x10, 0xFB, 0x2C, 2, 0x20, 0x10, 0xFB,
    ]);
    for prg in 0..4u8 {
        for chr in 0..4u8 {
            write(&mut program, 0x8000, prg << 4 | chr);
            observe(&mut program, 0x300 + u16::from(prg * 4 + chr));
        }
    }
    write(&mut program, 0x8000, 0);
    write(&mut program, 0xA000, 0x33); // $11 in old PRG => PRG 1, CHR 1
    observe(&mut program, 0x320);
    write(&mut program, 0xA000, 0x33); // $22 in old PRG => PRG 2, CHR 2
    observe(&mut program, 0x321);
    write(&mut program, 0x3FF, 0xA5);
    let stop = 0x8100 + u16::try_from(program.len()).expect("short program");
    program.extend_from_slice(&[0x4C, stop as u8, (stop >> 8) as u8]);
    assert!(program.len() < 0xF00);
    let mut image = vec![0xFF; 16 + 4 * 32768 + 4 * 8192];
    image[..16].fill(0);
    image[..4].copy_from_slice(b"NES\x1a");
    image[4] = 8;
    image[5] = 4;
    image[6] = 0x20;
    image[7] = 0x40;
    for bank in 0..4usize {
        let base = 16 + bank * 32768;
        image[base + 256..base + 256 + program.len()].copy_from_slice(&program);
        image[base + 0x1000] = 0x80 + bank as u8;
        image[base + 0x2000] = [0x11, 0x22, 0xFF, 0xFF][bank];
        image[base + 0x7FFC..base + 0x7FFE].copy_from_slice(&[0, 0x81]);
        image[16 + 4 * 32768 + bank * 8192..16 + 4 * 32768 + (bank + 1) * 8192]
            .fill(0xA0 + bank as u8);
    }
    image
}
