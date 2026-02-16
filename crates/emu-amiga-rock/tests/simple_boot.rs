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
    // 5. MOVE.W #$0F00, $DFF180 ($33FC 0F00 00DF F180) -- COLOR00 = Red
    rom[18] = 0x33; rom[19] = 0xFC; rom[20] = 0x0F; rom[21] = 0x00; 
    rom[22] = 0x00; rom[23] = 0xDF; rom[24] = 0xF1; rom[25] = 0x80;
    
    // Test DMA Setup:
    // 6. MOVE.W #$0038, $DFF092 (DDFSTRT = $38)
    rom[26] = 0x33; rom[27] = 0xFC; rom[28] = 0x00; rom[29] = 0x38;
    rom[30] = 0x00; rom[31] = 0xDF; rom[32] = 0xF0; rom[33] = 0x92;
    // 7. MOVE.W #$00D0, $DFF094 (DDFSTOP = $D0)
    rom[34] = 0x33; rom[35] = 0xFC; rom[36] = 0x00; rom[37] = 0xD0;
    rom[38] = 0x00; rom[39] = 0xDF; rom[40] = 0xF0; rom[41] = 0x94;
    // 8. MOVE.W #$8380, $DFF096 (DMACON = SET + DMAEN + BPLEN + COPEN)
    rom[42] = 0x33; rom[43] = 0xFC; rom[44] = 0x83; rom[45] = 0x80;
    rom[46] = 0x00; rom[47] = 0xDF; rom[48] = 0xF0; rom[49] = 0x96;
    // 9. MOVE.W #$1200, $DFF100 (BPLCON0 = 1 bitplane)
    rom[50] = 0x33; rom[51] = 0xFC; rom[52] = 0x12; rom[53] = 0x00;
    rom[54] = 0x00; rom[55] = 0xDF; rom[56] = 0xF1; rom[57] = 0x00;
    // 10. MOVE.L #$00002000, $DFF0E0 (BPL1PT = $2000)
    rom[58] = 0x23; rom[59] = 0xFC; rom[60] = 0x00; rom[61] = 0x00;
    rom[62] = 0x20; rom[63] = 0x00; rom[64] = 0x00; rom[65] = 0xDF;
    rom[66] = 0xF0; rom[67] = 0xE0;
    
    // Test Copper Setup:
    // 11. MOVE.L #$00003000, $DFF080 (COP1LC = $3000)
    rom[68] = 0x23; rom[69] = 0xFC; rom[70] = 0x00; rom[71] = 0x00;
    rom[72] = 0x30; rom[73] = 0x00; rom[74] = 0x00; rom[75] = 0xDF;
    rom[76] = 0xF0; rom[77] = 0x80;
    // 12. MOVE.W #$0000, $DFF088 (COPJMP1)
    rom[78] = 0x33; rom[79] = 0xFC; rom[80] = 0x00; rom[81] = 0x00;
    rom[82] = 0x00; rom[83] = 0xDF; rom[84] = 0xF0; rom[85] = 0x88;
    // 13. NOP
    rom[86] = 0x4E; rom[87] = 0x71;

    let mut amiga = Amiga::new(rom);
    
    // Initialize registers
    amiga.cpu.regs.a[0] = 0x00F80000; // Points to SSP value ($0008)
    amiga.cpu.regs.a[1] = 0x00001000; // Points to Chip RAM
    
    // Set some test data at $2000 in Chip RAM for DMA
    for i in 0..5120 {
        amiga.memory.write_byte(0x2000 + i*2, 0xAA);
        amiga.memory.write_byte(0x2001 + i*2, 0x55);
    }
    
    // Set Copper list at $3000
    // 1. WAIT v=50, h=0
    amiga.memory.write_byte(0x3000, 0x32); amiga.memory.write_byte(0x3001, 0x01); // 0x3201
    amiga.memory.write_byte(0x3002, 0xFF); amiga.memory.write_byte(0x3003, 0xFE); // 0xFFFE
    // 2. MOVE #$000F, COLOR00 (Blue)
    amiga.memory.write_byte(0x3004, 0x01); amiga.memory.write_byte(0x3005, 0x80); // 0x0180
    amiga.memory.write_byte(0x3006, 0x00); amiga.memory.write_byte(0x3007, 0x0F); // 0x000F
    // 3. END
    amiga.memory.write_byte(0x3008, 0xFF); amiga.memory.write_byte(0x3009, 0xFF);
    amiga.memory.write_byte(0x300A, 0xFF); amiga.memory.write_byte(0x300B, 0xFE);

    // Run for longer now (100,000 ticks) to let the beam reach line 50
    for _ in 0..100000 {
        amiga.tick();
    }

    // Check basic results
    amiga.memory.overlay = false;
    assert_eq!(amiga.cpu.regs.d[1] & 0xFFFF, 8);
    assert_eq!(amiga.cpu.regs.d[2] & 0xFFFF, 0x1234);
    assert_eq!(amiga.memory.read_byte(0x1000), 0x00);
    assert_eq!(amiga.memory.read_byte(0x1001), 0x08);
    
    // Check DMA result
    assert_eq!(amiga.denise.bpl_data[0], 0xAA55);
    
    // Check Copper result
    // Should be Blue ($000F) now, overwritten the Red ($0F00) set by CPU
    assert_eq!(amiga.denise.palette[0], 0x000F);
}
