//! A synthetic TOS for the runtime tests, hand-assembled: reset vectors, a
//! program at `$FC0008` and a VBL handler at `$FC0100`.

/// The program's loop counter; the VBL handler's frame counter follows it.
pub const COUNTER: u32 = 0x1_0000;

/// The program points the VBL vector at the handler, turns channel A of the
/// YM2149 up, lowers the interrupt mask to 3 and counts passes of a loop at
/// [`COUNTER`]; the handler counts frames at `COUNTER + 2`. A snapshot thus
/// covers the CPU, the interrupt path, video timing and sound.
pub fn tos() -> Vec<u8> {
    let mut rom = vec![0u8; 0x3_0000];
    rom[0..4].copy_from_slice(&0x0000_8000u32.to_be_bytes());
    rom[4..8].copy_from_slice(&0x00FC_0008u32.to_be_bytes());
    let long = |words: &mut Vec<u16>, v: u32| words.extend([(v >> 16) as u16, v as u16]);

    let mut program: Vec<u16> = Vec::new();
    // MOVE.L #$FC0100,$70.W: the VBL vector.
    program.push(0x21FC);
    long(&mut program, 0xFC_0100);
    program.push(0x0070);
    // MOVE.B #8,$FF8800; MOVE.B #15,$FF8802: channel A at full volume.
    for (byte, port) in [(8, 0xFF_8800), (15, 0xFF_8802)] {
        program.extend([0x13FC, byte]);
        long(&mut program, port);
    }
    // MOVE.W #$2300,SR
    program.extend([0x46FC, 0x2300]);
    // loop: ADDQ.W #1,COUNTER; BRA.S loop
    program.push(0x5279);
    long(&mut program, COUNTER);
    program.push(0x60F8);

    let mut handler: Vec<u16> = Vec::new();
    // ADDQ.W #1,COUNTER+2; RTE
    handler.push(0x5279);
    long(&mut handler, COUNTER + 2);
    handler.push(0x4E73);

    for (at, words) in [(0x08, &program), (0x100, &handler)] {
        for (i, w) in words.iter().enumerate() {
            rom[at + i * 2..at + i * 2 + 2].copy_from_slice(&w.to_be_bytes());
        }
    }
    rom
}

/// A big-endian word as the CPU sees it.
#[allow(dead_code)]
pub fn word(machine: &runtime_atari_st::AtariSt, at: u32) -> u16 {
    u16::from(machine.peek(at)) << 8 | u16::from(machine.peek(at + 1))
}

/// EmuTOS 1.3 (192 KB, US) from the test-data cache, if it is there.
#[allow(dead_code)]
pub fn emutos() -> Option<Vec<u8>> {
    let home = std::env::var_os("HOME")?;
    let path = std::path::Path::new(&home).join(".emu198x/test-data/emutos/etos192us.img");
    std::fs::read(path).ok()
}
