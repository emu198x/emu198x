//! A synthetic cartridge for the runtime tests, hand-assembled: a 32 KB
//! headerless LoROM whose program turns on NMI and auto-read, then counts
//! loop passes in `$10` while its NMI handler counts frames in `$11` and
//! keeps the pad in `$12-$13`.

/// Work RAM offsets the program writes.
pub const LOOP_COUNT: usize = 0x10;
pub const FRAME_COUNT: usize = 0x11;
pub const JOY1: usize = 0x12;

pub fn cartridge() -> Vec<u8> {
    let mut rom = vec![0u8; 0x8000];
    let program: &[u8] = &[
        0xA9, 0x81, // LDA #$81
        0x8D, 0x00, 0x42, // STA NMITIMEN
        0xEE, 0x10, 0x00, // loop: INC $0010
        0x80, 0xFB, // BRA loop
    ];
    let handler: &[u8] = &[
        0xAD, 0x10, 0x42, // LDA RDNMI
        0xEE, 0x11, 0x00, // INC $0011
        0xAD, 0x12, 0x42, // wait: LDA HVBJOY
        0x29, 0x01, // AND #1
        0xD0, 0xF9, // BNE wait
        0xAD, 0x18, 0x42, // LDA JOY1L
        0x8D, 0x12, 0x00, // STA $0012
        0xAD, 0x19, 0x42, // LDA JOY1H
        0x8D, 0x13, 0x00, // STA $0013
        0x40, // RTI
    ];
    rom[..program.len()].copy_from_slice(program);
    rom[0x100..0x100 + handler.len()].copy_from_slice(handler);
    rom[0x7FFC..0x7FFE].copy_from_slice(&0x8000u16.to_le_bytes());
    rom[0x7FFA..0x7FFC].copy_from_slice(&0x8100u16.to_le_bytes());
    rom
}

/// A cartridge with a proper header for `destination`, so region
/// detection has something to read.
#[allow(dead_code)]
pub fn headed_cartridge(destination: u8) -> Vec<u8> {
    let mut rom = cartridge();
    let base = 0x7FC0;
    rom[base..base + 21].copy_from_slice(b"EMU198X RUNTIME TEST ");
    rom[base + 0x15] = 0x20;
    rom[base + 0x17] = 0x05;
    rom[base + 0x19] = destination;
    let checksum = format_nintendo_snes_sfc::compute_checksum(&rom);
    rom[base + 0x1C..base + 0x1E].copy_from_slice(&(!checksum).to_le_bytes());
    rom[base + 0x1E..base + 0x20].copy_from_slice(&checksum.to_le_bytes());
    rom
}
