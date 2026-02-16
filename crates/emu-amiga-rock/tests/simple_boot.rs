//! Simple boot test for emu-amiga-rock.

use emu_amiga_rock::Amiga;

#[test]
fn test_minimal_execution() {
    let mut rom = vec![0u8; 256 * 1024];
    
    // Initial SSP = $00080000
    rom[0] = 0x00; rom[1] = 0x08; rom[2] = 0x00; rom[3] = 0x00;
    // Initial PC = $00F80008
    rom[4] = 0x00; rom[5] = 0xF8; rom[6] = 0x00; rom[7] = 0x08;
    
    // Instructions at $00F80008:
    // 1. NOP ($4E71)
    rom[8] = 0x4E; rom[9] = 0x71;
    // 2. MOVE.W (A0), D1 ($3210)
    rom[10] = 0x32; rom[11] = 0x10;
    // 3. MOVE.W #$1234, D2 ($343C 1234)
    rom[12] = 0x34; rom[13] = 0x3C; rom[14] = 0x12; rom[15] = 0x34;
    // 4. MOVE.W D1, (A1) ($3281)
    rom[16] = 0x32; rom[17] = 0x81;
    // 5. RESET ($4E70)
    rom[18] = 0x4E; rom[19] = 0x70;

    let mut amiga = Amiga::new(rom);
    
    // Initialize registers
    amiga.cpu.regs.a[0] = 0x00F80000; // Points to SSP value ($0008)
    amiga.cpu.regs.a[1] = 0x00001000; // Points to Chip RAM

    // Run for a bit longer now (1000 ticks)
    for _ in 0..1000 {
        amiga.tick();
    }

    // Check results
    amiga.memory.overlay = false; // Disable overlay to check actual RAM
    let d1 = amiga.cpu.regs.d[1];
    let d2 = amiga.cpu.regs.d[2];
    let m1000 = amiga.memory.read_byte(0x1000);
    let m1001 = amiga.memory.read_byte(0x1001);

    // D1 contains $0008
    assert_eq!(d1 & 0xFFFF, 8);
    // D2 contains $1234
    assert_eq!(d2 & 0xFFFF, 0x1234);
    // Chip RAM at $1000 contains $0008
    assert_eq!(m1000, 0x00);
    assert_eq!(m1001, 0x08);
}
