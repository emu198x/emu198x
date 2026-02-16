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
    let mut p = 8;
    let mut write = |bytes: &[u8]| {
        for &b in bytes {
            rom[p] = b;
            p += 1;
        }
    };

    // 1. NOP
    write(&[0x4E, 0x71]);
    // 2. MOVE.W (A0), D1
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
    // 10. MOVE.W #$0038, $DFF092
    write(&[0x33, 0xFC, 0x00, 0x38, 0x00, 0xDF, 0xF0, 0x92]);
    // 11. MOVE.W #$00D0, $DFF094
    write(&[0x33, 0xFC, 0x00, 0xD0, 0x00, 0xDF, 0xF0, 0x94]);
    // 12. MOVE.W #$8380, $DFF096
    write(&[0x33, 0xFC, 0x83, 0x80, 0x00, 0xDF, 0xF0, 0x96]);
    // 13. MOVE.W #$1200, $DFF100
    write(&[0x33, 0xFC, 0x12, 0x00, 0x00, 0xDF, 0xF1, 0x00]);
    // 14. MOVE.L #$00002000, $DFF0E0
    write(&[0x23, 0xFC, 0x00, 0x00, 0x20, 0x00, 0x00, 0xDF, 0xF0, 0xE0]);
    // 15. MOVE.L #$00003000, $DFF080
    write(&[0x23, 0xFC, 0x00, 0x00, 0x30, 0x00, 0x00, 0xDF, 0xF0, 0x80]);
    // 16. MOVE.W #$0000, $DFF088
    write(&[0x33, 0xFC, 0x00, 0x00, 0x00, 0xDF, 0xF0, 0x88]);
    // 17. MOVE.B #$01, $BFE201
    write(&[0x13, 0xFC, 0x00, 0x01, 0x00, 0xBF, 0xE2, 0x01]);
    // 18. MOVE.B #$00, $BFE001
    write(&[0x13, 0xFC, 0x00, 0x00, 0x00, 0xBF, 0xE0, 0x01]);
    // 19. NOP
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

    // Run for longer now (500,000 ticks)
    for _ in 0..500000 {
        amiga.tick();
    }

    // Check basic results
    amiga.memory.overlay = false;
    assert_eq!(amiga.cpu.regs.d[1] & 0xFFFF, 8);
    assert_eq!(amiga.cpu.regs.d[2] & 0xFFFF, 0x122C);
    assert_eq!(amiga.memory.overlay, false);
    
    // Check Copper result
    assert_eq!(amiga.denise.palette[0], 0x000F);
    
    // Check DMA and Pixel Shifter
    // Palette[0] is Blue ($000F) -> 0xFF0000FF
    // Palette[1] is White ($0FFF) -> 0xFFFFFFFF
    // $AA55 = 10101010 01010101
    // Pixel 0 should be index 1 (White)
    // Pixel 1 should be index 0 (Blue)
    
    // Find a coordinate that was definitely drawn
    // Vpos 50, Hpos 0x38 (DDFSTRT) -> FB_Y = 50 - 0x2C = 6, FB_X = (0x38 - 0x2E) * 2 = 20
    let px0 = amiga.denise.framebuffer[(6 * 320 + 20) as usize];
    let px1 = amiga.denise.framebuffer[(6 * 320 + 21) as usize];
    
    assert_eq!(px0, 0xFFFFFFFF); // White
    assert_eq!(px1, 0xFF0000FF); // Blue
}
