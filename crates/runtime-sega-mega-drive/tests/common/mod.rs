//! A synthetic cartridge for the runtime tests, hand-assembled like the
//! machine crate's: reset vectors, a program at `$200` and a V-int handler
//! at `$300`.

/// Work RAM.
pub const RAM: u32 = 0xFF_0000;

/// The program turns on V-ints, enables them in SR and then counts passes
/// of a loop at `RAM`; the handler counts frames at `RAM + 2` and plays a
/// PSG tone, so the snapshot covers both CPUs' views, the VDP and sound.
pub fn cartridge() -> Vec<u8> {
    let mut rom = vec![0u8; 0x800];
    rom[0..4].copy_from_slice(&0x00FF_FE00u32.to_be_bytes());
    rom[4..8].copy_from_slice(&0x200u32.to_be_bytes());
    rom[0x78..0x7C].copy_from_slice(&0x300u32.to_be_bytes());
    let long = |words: &mut Vec<u16>, v: u32| words.extend([(v >> 16) as u16, v as u16]);

    let mut program: Vec<u16> = Vec::new();
    // MOVE.W #$8164,$C00004: display and V-int on.
    program.extend([0x33FC, 0x8164]);
    long(&mut program, 0xC0_0004);
    // MOVE.B #$80,$C00011; MOVE.B #$01,...; MOVE.B #$90,...: a PSG tone.
    for byte in [0x80, 0x01, 0x90] {
        program.extend([0x13FC, byte]);
        long(&mut program, 0xC0_0011);
    }
    // MOVE.W #$2000,SR
    program.extend([0x46FC, 0x2000]);
    // loop: ADDQ.W #1,RAM; BRA.S loop
    program.push(0x5279);
    long(&mut program, RAM);
    program.push(0x60F8);

    let mut handler: Vec<u16> = Vec::new();
    // ADDQ.W #1,RAM+2; RTE
    handler.push(0x5279);
    long(&mut handler, RAM + 2);
    handler.push(0x4E73);

    for (at, words) in [(0x200, &program), (0x300, &handler)] {
        for (i, w) in words.iter().enumerate() {
            rom[at + i * 2..at + i * 2 + 2].copy_from_slice(&w.to_be_bytes());
        }
    }
    rom
}

/// A big-endian word of work RAM.
#[allow(dead_code)]
pub fn ram_word(ram: &[u8], at: u32) -> u16 {
    let a = (at & 0xFFFF) as usize;
    u16::from(ram[a]) << 8 | u16::from(ram[a + 1])
}
