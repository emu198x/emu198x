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
    // 3. RESET ($4E70)
    rom[12] = 0x4E; rom[13] = 0x70;

    let mut amiga = Amiga::new(rom);
    
    // Initialize A0 to point to some data in ROM
    amiga.cpu.regs.a[0] = 0x00F80000; // Points to SSP value ($0008)

    // Run for a few hundred crystal ticks
    // 1 instruction is roughly 4-12 CPU cycles = 16-48 crystal ticks.
    for _ in 0..500 {
        amiga.tick();
    }

    // After 500 ticks, we expect:
    // - NOP executed
    // - MOVE.W (A0), D1 executed
    // - RESET executed (or at least started)
    
    // Check D1 contains the high word of SSP ($0008)
    assert_eq!(amiga.cpu.regs.d[1] & 0xFFFF, 0x0008);
    
    println!("D1 low word: ${:04X}", amiga.cpu.regs.d[1] & 0xFFFF);
    println!("CPU state: {:?}", match amiga.cpu.state {
        cpu_m68k_rock::cpu::State::Idle => "Idle",
        cpu_m68k_rock::cpu::State::Internal { .. } => "Internal",
        cpu_m68k_rock::cpu::State::BusCycle { .. } => "BusCycle",
        cpu_m68k_rock::cpu::State::Halted => "Halted",
        cpu_m68k_rock::cpu::State::Stopped => "Stopped",
    });
}
