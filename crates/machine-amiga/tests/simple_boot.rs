//! Simple boot test for machine-amiga.

use machine_amiga::Amiga;

#[test]
fn test_minimal_execution() {
    let mut rom = vec![0u8; 256 * 1024];
    
    // Initial SSP = $00080000
    rom[0] = 0x00; rom[1] = 0x08; rom[2] = 0x00; rom[3] = 0x00;
    // Initial PC = $00F80008
    rom[4] = 0x00; rom[5] = 0xF8; rom[6] = 0x00; rom[7] = 0x08;
    
    // Instructions at $00F80008:
    let mut p = 8;
    let mut write = |bytes: &[u8]| {
        for &b in bytes {
            rom[p] = b;
            p += 1;
        }
    };

    // 1. NOP
    write(&[0x4E, 0x71]);
    // 2. MOVE.W (A0), D1 -> D1 = 8
    write(&[0x32, 0x10]);
    // 3. MOVE.W #$1234, D2
    write(&[0x34, 0x3C, 0x12, 0x34]);
    // 4. SUB.W D1, D2 -> $1234 - 8 = $122C
    write(&[0x94, 0x41]);
    // 5. CMP.W #$122C, D2 -> Should set Z flag
    write(&[0xB4, 0x3C, 0x12, 0x2C]);
    // 6. BEQ.S +8 (Skip to Green)
    write(&[0x67, 0x08]);
    // 7. MOVE.W #$0F00, $DFF180 (Red) -- Should be skipped!
    write(&[0x33, 0xFC, 0x0F, 0x00, 0x00, 0xDF, 0xF1, 0x80]);
    // 8. MOVE.W #$00F0, $DFF180 (Green)
    write(&[0x33, 0xFC, 0x00, 0xF0, 0x00, 0xDF, 0xF1, 0x80]);
    // 9. MOVE.W #$0FFF, $DFF182 (COLOR01 = White)
    write(&[0x33, 0xFC, 0x0F, 0xFF, 0x00, 0xDF, 0xF1, 0x82]);
    // 10. BTST #3, D1 -> Bit 3 of 8 is 1, so Z=0
    write(&[0x08, 0x01, 0x00, 0x03]);
    // 11. BSET #4, D1 -> D1 becomes 8 + 16 = 24
    write(&[0x08, 0xC1, 0x00, 0x04]);
    // 12. MOVE.W #$0038, $DFF092
    write(&[0x33, 0xFC, 0x00, 0x38, 0x00, 0xDF, 0xF0, 0x92]);
    // 13. MOVE.W #$00D0, $DFF094
    write(&[0x33, 0xFC, 0x00, 0xD0, 0x00, 0xDF, 0xF0, 0x94]);
    // 14. MOVE.W #$8380, $DFF096
    write(&[0x33, 0xFC, 0x83, 0x80, 0x00, 0xDF, 0xF0, 0x96]);
    // 15. MOVE.W #$1200, $DFF100
    write(&[0x33, 0xFC, 0x12, 0x00, 0x00, 0xDF, 0xF1, 0x00]);
    // 16. MOVE.L #$00002000, $DFF0E0
    write(&[0x23, 0xFC, 0x00, 0x00, 0x20, 0x00, 0x00, 0xDF, 0xF0, 0xE0]);
    // 17. MOVE.L #$00003000, $DFF080
    write(&[0x23, 0xFC, 0x00, 0x00, 0x30, 0x00, 0x00, 0xDF, 0xF0, 0x80]);
    // 18. MOVE.W #$0000, $DFF088
    write(&[0x33, 0xFC, 0x00, 0x00, 0x00, 0xDF, 0xF0, 0x88]);
    // 19. MOVE.B #$01, $BFE201
    write(&[0x13, 0xFC, 0x00, 0x01, 0x00, 0xBF, 0xE2, 0x01]);
    // 20. MOVE.B #$00, $BFE001
    write(&[0x13, 0xFC, 0x00, 0x00, 0x00, 0xBF, 0xE0, 0x01]);
    // 21. NOP
    write(&[0x4E, 0x71]);

    let mut amiga = Amiga::new(rom);
    
    // Initialize registers
    amiga.cpu.regs.a[0] = 0x00F80000;
    amiga.cpu.regs.a[1] = 0x00001000;
    
    // Set some test data at $2000 in Chip RAM for DMA
    for i in 0..5120 {
        amiga.memory.write_byte(0x2000 + i*2, 0xAA); // 10101010
        amiga.memory.write_byte(0x2001 + i*2, 0x55); // 01010101
    }
    
    // Set Copper list at $3000
    amiga.memory.write_byte(0x3000, 0x32); amiga.memory.write_byte(0x3001, 0x01); // WAIT v=50, h=0
    amiga.memory.write_byte(0x3002, 0xFF); amiga.memory.write_byte(0x3003, 0xFE);
    amiga.memory.write_byte(0x3004, 0x01); amiga.memory.write_byte(0x3005, 0x80); // MOVE #$000F, COLOR00
    amiga.memory.write_byte(0x3006, 0x00); amiga.memory.write_byte(0x3007, 0x0F);
    amiga.memory.write_byte(0x3008, 0xFF); amiga.memory.write_byte(0x3009, 0xFF); // END
    amiga.memory.write_byte(0x300A, 0xFF); amiga.memory.write_byte(0x300B, 0xFE);

    // Run for longer now (300,000 ticks)
    for _ in 0..300000 {
        amiga.tick();
    }

    // Check basic results
    amiga.memory.overlay = false;
    assert_eq!(amiga.cpu.regs.d[1] & 0xFF, 24); // BSET worked
    assert_eq!(amiga.cpu.regs.d[2] & 0xFFFF, 0x122C);
    
    // Check Copper result
    assert_eq!(amiga.denise.palette[0], 0x000F);
    
    // Check DMA and Pixel Shifter
    let px0 = amiga.denise.framebuffer[(6 * 320 + 20) as usize];
    let px1 = amiga.denise.framebuffer[(6 * 320 + 21) as usize];
    assert_eq!(px0, 0xFFFFFFFF); // White
    assert_eq!(px1, 0xFF0000FF); // Blue
    
    // Check CIA-A Result (Overlay off)
    assert_eq!(amiga.memory.overlay, false);
}
