//! Basic instruction tests for the 68000 CPU.

use emu_68000::M68000;
use emu_core::{Cpu, Observable, SimpleBus};

/// Helper to load words into memory (big-endian).
fn load_words(bus: &mut SimpleBus, addr: u16, words: &[u16]) {
    for (i, &word) in words.iter().enumerate() {
        let base = addr as usize + i * 2;
        bus.poke(base as u16, (word >> 8) as u8);
        bus.poke((base + 1) as u16, word as u8);
    }
}

/// Run the CPU until the micro-op queue is ready for the next instruction.
fn run_instruction(cpu: &mut M68000, bus: &mut SimpleBus) -> u32 {
    let mut cycles = 0u32;
    let max_cycles = 200;

    // Run at least one tick
    cpu.tick(bus);
    cycles += 1;

    // Continue until ready for next instruction
    while cycles < max_cycles {
        cpu.tick(bus);
        cycles += 1;

        // Check if we're at the start of a new instruction fetch
        if cycles > 4 {
            // Simple heuristic: if we've run more than one instruction's worth
            // and we're back at the beginning, we're done
            break;
        }
    }

    cycles
}

#[test]
fn test_cpu_creation() {
    let cpu = M68000::new();

    // Check initial state
    assert_eq!(cpu.query("d0"), Some(emu_core::Value::U32(0)));
    assert_eq!(cpu.query("pc"), Some(emu_core::Value::U32(0)));
    assert!(cpu.query("flags.s") == Some(emu_core::Value::Bool(true))); // Supervisor mode
}

#[test]
fn test_moveq() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MOVEQ #$42, D0 (opcode: 0x7042)
    load_words(&mut bus, 0x1000, &[0x7042]);
    cpu.reset();
    cpu.regs.pc = 0x1000;

    run_instruction(&mut cpu, &mut bus);

    assert_eq!(cpu.regs.d[0], 0x42);
}

#[test]
fn test_moveq_negative() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MOVEQ #-1, D0 (opcode: 0x70FF)
    load_words(&mut bus, 0x1000, &[0x70FF]);
    cpu.reset();
    cpu.regs.pc = 0x1000;

    run_instruction(&mut cpu, &mut bus);

    assert_eq!(cpu.regs.d[0], 0xFFFF_FFFF); // Sign extended to 32 bits
}

#[test]
fn test_nop() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // NOP (opcode: 0x4E71)
    load_words(&mut bus, 0x1000, &[0x4E71]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x1234;

    run_instruction(&mut cpu, &mut bus);

    // NOP should not change any registers
    assert_eq!(cpu.regs.d[0], 0x1234);
}

#[test]
fn test_exg_data_registers() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // EXG D0, D1 (opcode: 0xC141)
    load_words(&mut bus, 0x1000, &[0xC141]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x1111_1111;
    cpu.regs.d[1] = 0x2222_2222;

    run_instruction(&mut cpu, &mut bus);

    assert_eq!(cpu.regs.d[0], 0x2222_2222);
    assert_eq!(cpu.regs.d[1], 0x1111_1111);
}

#[test]
fn test_swap() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // SWAP D0 (opcode: 0x4840)
    load_words(&mut bus, 0x1000, &[0x4840]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x1234_5678;

    run_instruction(&mut cpu, &mut bus);

    assert_eq!(cpu.regs.d[0], 0x5678_1234);
}

#[test]
fn test_ext_word() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // EXT.W D0 (opcode: 0x4880)
    load_words(&mut bus, 0x1000, &[0x4880]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_00FF; // -1 as a byte

    run_instruction(&mut cpu, &mut bus);

    // Low word should be sign-extended from byte
    assert_eq!(cpu.regs.d[0] & 0xFFFF, 0xFFFF);
}

#[test]
fn test_ext_long() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // EXT.L D0 (opcode: 0x48C0)
    load_words(&mut bus, 0x1000, &[0x48C0]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_FFFF; // -1 as a word

    run_instruction(&mut cpu, &mut bus);

    // Should be sign-extended from word to long
    assert_eq!(cpu.regs.d[0], 0xFFFF_FFFF);
}

#[test]
fn test_observable_registers() {
    let cpu = M68000::new();

    // Check that all documented query paths work
    let paths = cpu.query_paths();
    assert!(paths.contains(&"d0"));
    assert!(paths.contains(&"a7"));
    assert!(paths.contains(&"pc"));
    assert!(paths.contains(&"sr"));
    assert!(paths.contains(&"flags.z"));
}

#[test]
fn test_move_data_to_data() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MOVE.L D1, D0 (opcode: 0x2001)
    load_words(&mut bus, 0x1000, &[0x2001]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[1] = 0xDEAD_BEEF;

    run_instruction(&mut cpu, &mut bus);

    assert_eq!(cpu.regs.d[0], 0xDEAD_BEEF);
}

#[test]
fn test_move_immediate_to_data() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MOVE.L #$12345678, D0 (opcode: 0x203C, followed by immediate)
    load_words(&mut bus, 0x1000, &[0x203C, 0x1234, 0x5678]);
    cpu.reset();
    cpu.regs.pc = 0x1000;

    // Run more cycles for this longer instruction
    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    assert_eq!(cpu.regs.d[0], 0x1234_5678);
}

#[test]
fn test_move_word_immediate() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MOVE.W #$ABCD, D0 (opcode: 0x303C, followed by immediate)
    load_words(&mut bus, 0x1000, &[0x303C, 0xABCD]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x1111_2222;

    for _ in 0..16 {
        cpu.tick(&mut bus);
    }

    // Word move should only affect low word
    assert_eq!(cpu.regs.d[0], 0x1111_ABCD);
}

#[test]
fn test_move_addr_indirect() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MOVE.L (A0), D0 (opcode: 0x2010)
    load_words(&mut bus, 0x1000, &[0x2010]);
    // Put data at address pointed by A0
    load_words(&mut bus, 0x2000, &[0xCAFE, 0xBABE]);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    assert_eq!(cpu.regs.d[0], 0xCAFE_BABE);
}

#[test]
fn test_move_postinc() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MOVE.L (A0)+, D0 (opcode: 0x2018)
    load_words(&mut bus, 0x1000, &[0x2018]);
    load_words(&mut bus, 0x2000, &[0x1234, 0x5678]);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    assert_eq!(cpu.regs.d[0], 0x1234_5678);
    assert_eq!(cpu.regs.a(0), 0x2004); // A0 should be incremented by 4
}

#[test]
fn test_move_predec() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MOVE.L -(A0), D0 (opcode: 0x2020)
    load_words(&mut bus, 0x1000, &[0x2020]);
    load_words(&mut bus, 0x1FFC, &[0xABCD, 0xEF01]);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    assert_eq!(cpu.regs.d[0], 0xABCD_EF01);
    assert_eq!(cpu.regs.a(0), 0x1FFC); // A0 should be decremented by 4
}

#[test]
fn test_movea_word() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MOVEA.W D0, A1 (opcode: 0x3240)
    load_words(&mut bus, 0x1000, &[0x3240]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_FFFF; // -1 as word

    run_instruction(&mut cpu, &mut bus);

    // MOVEA.W should sign-extend to 32 bits
    assert_eq!(cpu.regs.a(1), 0xFFFF_FFFF);
}

#[test]
fn test_move_to_memory() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MOVE.L D0, (A1) (opcode: 0x2280)
    load_words(&mut bus, 0x1000, &[0x2280]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x1234_5678;
    cpu.regs.set_a(1, 0x2000);

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Check memory was written
    let hi = u16::from(bus.peek(0x2000)) << 8 | u16::from(bus.peek(0x2001));
    let lo = u16::from(bus.peek(0x2002)) << 8 | u16::from(bus.peek(0x2003));
    let value = u32::from(hi) << 16 | u32::from(lo);
    assert_eq!(value, 0x1234_5678);
}

#[test]
fn test_addq_data_reg() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ADDQ.L #5, D0 (opcode: 0x5A80)
    // 0101 101 0 10 000 000 = 0x5A80
    load_words(&mut bus, 0x1000, &[0x5A80]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_0010;

    run_instruction(&mut cpu, &mut bus);

    assert_eq!(cpu.regs.d[0], 0x0000_0015); // 0x10 + 5 = 0x15
}

#[test]
fn test_addq_data_8() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ADDQ.L #8, D0 (opcode: 0x5080)
    // 0101 000 0 10 000 000 = 0x5080 (data=0 means 8)
    load_words(&mut bus, 0x1000, &[0x5080]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_0000;

    run_instruction(&mut cpu, &mut bus);

    assert_eq!(cpu.regs.d[0], 0x0000_0008); // 0 + 8 = 8
}

#[test]
fn test_addq_addr_reg() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ADDQ.L #3, A0 (opcode: 0x5648)
    // 0101 011 0 01 001 000 = 0x5648
    load_words(&mut bus, 0x1000, &[0x5648]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x0000_1000);

    run_instruction(&mut cpu, &mut bus);

    assert_eq!(cpu.regs.a(0), 0x0000_1003); // 0x1000 + 3 = 0x1003
}

#[test]
fn test_subq_data_reg() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // SUBQ.L #5, D0 (opcode: 0x5B80)
    // 0101 101 1 10 000 000 = 0x5B80
    load_words(&mut bus, 0x1000, &[0x5B80]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_0015;

    run_instruction(&mut cpu, &mut bus);

    assert_eq!(cpu.regs.d[0], 0x0000_0010); // 0x15 - 5 = 0x10
}

#[test]
fn test_subq_addr_reg() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // SUBQ.L #4, A1 (opcode: 0x5989)
    // 0101 100 1 10 001 001 = 0x5989
    load_words(&mut bus, 0x1000, &[0x5989]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(1, 0x0000_2000);

    run_instruction(&mut cpu, &mut bus);

    assert_eq!(cpu.regs.a(1), 0x0000_1FFC); // 0x2000 - 4 = 0x1FFC
}

#[test]
fn test_addq_memory_word() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ADDQ.W #3, (A0) (opcode: 0x5650)
    // 0101 011 0 01 010 000 = 0x5650
    // NOP to prevent garbage execution
    load_words(&mut bus, 0x1000, &[0x5650, 0x4E71]);
    load_words(&mut bus, 0x2000, &[0x0010]); // Memory contains 0x0010
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.a[0] = 0x2000;

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Memory should now contain 0x0010 + 3 = 0x0013
    let hi = bus.peek(0x2000);
    let lo = bus.peek(0x2001);
    let result = u16::from(hi) << 8 | u16::from(lo);
    assert_eq!(result, 0x0013);
}

#[test]
fn test_subq_memory_byte() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // SUBQ.B #2, (A0) (opcode: 0x5510)
    // 0101 010 1 00 010 000 = 0x5510
    // NOP to prevent garbage execution
    load_words(&mut bus, 0x1000, &[0x5510, 0x4E71]);
    bus.poke(0x2000, 0x10); // Memory contains 0x10
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.a[0] = 0x2000;

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Memory should now contain 0x10 - 2 = 0x0E
    assert_eq!(bus.peek(0x2000), 0x0E);
}

#[test]
fn test_not() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // NOT.L D0 (opcode: 0x4680)
    // 0100 0110 10 000 000 = 0x4680
    load_words(&mut bus, 0x1000, &[0x4680]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_FFFF;

    run_instruction(&mut cpu, &mut bus);

    assert_eq!(cpu.regs.d[0], 0xFFFF_0000); // NOT of 0x0000FFFF
}

#[test]
fn test_and_registers() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // AND.L D1, D0 (opcode: 0xC081)
    // 1100 000 0 10 000 001 = 0xC081
    load_words(&mut bus, 0x1000, &[0xC081]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0xFFFF_0000;
    cpu.regs.d[1] = 0x00FF_FF00;

    run_instruction(&mut cpu, &mut bus);

    assert_eq!(cpu.regs.d[0], 0x00FF_0000); // AND result
}

#[test]
fn test_or_registers() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // OR.L D1, D0 (opcode: 0x8081)
    // 1000 000 0 10 000 001 = 0x8081
    load_words(&mut bus, 0x1000, &[0x8081]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0xFFFF_0000;
    cpu.regs.d[1] = 0x00FF_FF00;

    run_instruction(&mut cpu, &mut bus);

    assert_eq!(cpu.regs.d[0], 0xFFFF_FF00); // OR result
}

#[test]
fn test_eor_registers() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // EOR.L D0, D1 (opcode: 0xB181)
    // 1011 000 1 10 000 001 = 0xB181
    load_words(&mut bus, 0x1000, &[0xB181]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0xFFFF_0000;
    cpu.regs.d[1] = 0xF0F0_F0F0;

    run_instruction(&mut cpu, &mut bus);

    // D0 XOR D1 -> D1
    assert_eq!(cpu.regs.d[1], 0x0F0F_F0F0);
}

#[test]
fn test_tst() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // TST.L D0 (opcode: 0x4A80)
    // 0100 1010 10 000 000 = 0x4A80
    load_words(&mut bus, 0x1000, &[0x4A80]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_0000;

    run_instruction(&mut cpu, &mut bus);

    // Zero flag should be set
    assert!(cpu.query("flags.z") == Some(emu_core::Value::Bool(true)));
}

#[test]
fn test_lsl_immediate() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // LSL.L #4, D0 (opcode: 0xE988)
    // 1110 100 1 10 001 000 = 0xE988
    load_words(&mut bus, 0x1000, &[0xE988]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_000F;

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    assert_eq!(cpu.regs.d[0], 0x0000_00F0); // 0x0F << 4 = 0xF0
}

#[test]
fn test_lsr_immediate() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // LSR.L #4, D0 (opcode: 0xE888)
    // 1110 100 0 10 001 000 = 0xE888
    load_words(&mut bus, 0x1000, &[0xE888]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_00F0;

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    assert_eq!(cpu.regs.d[0], 0x0000_000F); // 0xF0 >> 4 = 0x0F
}

#[test]
fn test_asr_sign_extend() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ASR.L #4, D0 (opcode: 0xE880)
    // 1110 100 0 10 000 000 = 0xE880
    load_words(&mut bus, 0x1000, &[0xE880]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0xF000_0000; // Negative number

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // ASR preserves sign bit
    assert_eq!(cpu.regs.d[0], 0xFF00_0000);
}

#[test]
fn test_rol() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ROL.L #4, D0 (opcode: 0xE998)
    // 1110 100 1 10 011 000 = 0xE998
    load_words(&mut bus, 0x1000, &[0xE998]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x1234_5678;

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    assert_eq!(cpu.regs.d[0], 0x2345_6781); // Rotate left 4 bits
}

#[test]
fn test_bra_short() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // BRA.S $06 (opcode: 0x6006) - branch forward 6 bytes
    load_words(&mut bus, 0x1000, &[0x6006]);
    cpu.reset();
    cpu.regs.pc = 0x1000;

    run_instruction(&mut cpu, &mut bus);

    // PC should be 0x1002 + 6 = 0x1008
    assert_eq!(cpu.regs.pc, 0x1008);
}

#[test]
fn test_beq_taken() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // BEQ.S $04 (opcode: 0x6704) - branch if equal
    load_words(&mut bus, 0x1000, &[0x6704]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.sr |= emu_68000::Z; // Set zero flag

    run_instruction(&mut cpu, &mut bus);

    // Branch taken: PC = 0x1002 + 4 = 0x1006
    assert_eq!(cpu.regs.pc, 0x1006);
}

#[test]
fn test_beq_not_taken() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // BEQ.S $04 (opcode: 0x6704) - branch if equal
    load_words(&mut bus, 0x1000, &[0x6704]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.sr &= !emu_68000::Z; // Clear zero flag

    run_instruction(&mut cpu, &mut bus);

    // Branch not taken: PC = 0x1002 (after opcode)
    assert_eq!(cpu.regs.pc, 0x1002);
}

#[test]
fn test_bne_taken() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // BNE.S $04 (opcode: 0x6604) - branch if not equal
    load_words(&mut bus, 0x1000, &[0x6604]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.sr &= !emu_68000::Z; // Clear zero flag (not equal)

    run_instruction(&mut cpu, &mut bus);

    // Branch taken
    assert_eq!(cpu.regs.pc, 0x1006);
}

#[test]
fn test_dbf_terminates() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // DBF D0, label (opcode: 0x51C8 followed by displacement 0xFFFE)
    // DBF = DBRA = "decrement and branch always (if Dn != -1)"
    // condition F (false) means always decrement and check
    load_words(&mut bus, 0x1000, &[0x51C8, 0xFFFE]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0; // Will become -1 after decrement, so no branch

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // D0 should be 0xFFFF (-1 as word)
    assert_eq!(cpu.regs.d[0] & 0xFFFF, 0xFFFF);
    // PC should be past the instruction (no branch taken)
    assert_eq!(cpu.regs.pc, 0x1004);
}

#[test]
fn test_dbeq_condition_true() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // DBEQ D1, label (opcode: 0x57C9 followed by displacement)
    // condition EQ (Z=1) means "if equal, exit loop"
    load_words(&mut bus, 0x1000, &[0x57C9, 0xFFF8]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[1] = 5;
    cpu.regs.sr |= emu_68000::Z; // Set Z flag - condition true

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // D1 should NOT be decremented when condition is true
    assert_eq!(cpu.regs.d[1], 5);
    // PC should be past the instruction
    assert_eq!(cpu.regs.pc, 0x1004);
}

#[test]
fn test_btst_register() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // BTST D1, D0 (opcode: 0x0300)
    // 0000 001 100 000 000 = 0x0300
    load_words(&mut bus, 0x1000, &[0x0300]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_0004; // Bit 2 is set
    cpu.regs.d[1] = 2; // Test bit 2

    run_instruction(&mut cpu, &mut bus);

    // Bit 2 was 1, so Z should be clear
    assert!(cpu.query("flags.z") == Some(emu_core::Value::Bool(false)));
}

#[test]
fn test_btst_register_zero() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // BTST D1, D0
    load_words(&mut bus, 0x1000, &[0x0300]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_0004; // Bit 2 is set
    cpu.regs.d[1] = 3; // Test bit 3 (which is 0)

    run_instruction(&mut cpu, &mut bus);

    // Bit 3 was 0, so Z should be set
    assert!(cpu.query("flags.z") == Some(emu_core::Value::Bool(true)));
}

#[test]
fn test_bset_register() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // BSET D1, D0 (opcode: 0x03C0)
    // 0000 001 111 000 000 = 0x03C0
    load_words(&mut bus, 0x1000, &[0x03C0]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_0000;
    cpu.regs.d[1] = 4; // Set bit 4

    run_instruction(&mut cpu, &mut bus);

    assert_eq!(cpu.regs.d[0], 0x0000_0010); // Bit 4 now set
    // Z was set because bit was 0 before
    assert!(cpu.query("flags.z") == Some(emu_core::Value::Bool(true)));
}

#[test]
fn test_bclr_register() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // BCLR D1, D0 (opcode: 0x0380)
    // 0000 001 110 000 000 = 0x0380
    load_words(&mut bus, 0x1000, &[0x0380]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_00FF;
    cpu.regs.d[1] = 4; // Clear bit 4

    run_instruction(&mut cpu, &mut bus);

    assert_eq!(cpu.regs.d[0], 0x0000_00EF); // Bit 4 now clear (0xFF -> 0xEF)
    // Z was clear because bit was 1 before
    assert!(cpu.query("flags.z") == Some(emu_core::Value::Bool(false)));
}

#[test]
fn test_clr_long() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // CLR.L D0 (opcode: 0x4280)
    // 0100 0010 10 000 000 = 0x4280
    load_words(&mut bus, 0x1000, &[0x4280]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x1234_5678;

    run_instruction(&mut cpu, &mut bus);

    assert_eq!(cpu.regs.d[0], 0x0000_0000);
    // Z should be set
    assert!(cpu.query("flags.z") == Some(emu_core::Value::Bool(true)));
}

#[test]
fn test_clr_byte() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // CLR.B D0 (opcode: 0x4200)
    // 0100 0010 00 000 000 = 0x4200
    load_words(&mut bus, 0x1000, &[0x4200]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x1234_56FF;

    run_instruction(&mut cpu, &mut bus);

    // Only low byte should be cleared
    assert_eq!(cpu.regs.d[0], 0x1234_5600);
}

#[test]
fn test_neg_long() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // NEG.L D0 (opcode: 0x4480)
    // 0100 0100 10 000 000 = 0x4480
    load_words(&mut bus, 0x1000, &[0x4480]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_0001;

    run_instruction(&mut cpu, &mut bus);

    assert_eq!(cpu.regs.d[0], 0xFFFF_FFFF); // -1
}

#[test]
fn test_neg_word() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // NEG.W D0 (opcode: 0x4440)
    // 0100 0100 01 000 000 = 0x4440
    load_words(&mut bus, 0x1000, &[0x4440]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x1234_0005;

    run_instruction(&mut cpu, &mut bus);

    // Only low word should be negated
    assert_eq!(cpu.regs.d[0], 0x1234_FFFB); // 0 - 5 = -5 = 0xFFFB
}

#[test]
fn test_mulu() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MULU D1, D0 (opcode: 0xC0C1)
    // 1100 000 011 000 001 = 0xC0C1
    load_words(&mut bus, 0x1000, &[0xC0C1]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_0064; // 100
    cpu.regs.d[1] = 0x0000_000A; // 10

    for _ in 0..80 {
        cpu.tick(&mut bus);
    }

    assert_eq!(cpu.regs.d[0], 0x0000_03E8); // 100 * 10 = 1000
}

#[test]
fn test_muls() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MULS D1, D0 (opcode: 0xC1C1)
    // 1100 000 111 000 001 = 0xC1C1
    load_words(&mut bus, 0x1000, &[0xC1C1]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_FFFE; // -2 as word
    cpu.regs.d[1] = 0x0000_0005; // 5

    for _ in 0..80 {
        cpu.tick(&mut bus);
    }

    assert_eq!(cpu.regs.d[0], 0xFFFF_FFF6); // -2 * 5 = -10
}

#[test]
fn test_divu() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // DIVU D1, D0 (opcode: 0x80C1)
    // 1000 000 011 000 001 = 0x80C1
    load_words(&mut bus, 0x1000, &[0x80C1]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 100; // Dividend
    cpu.regs.d[1] = 7;   // Divisor

    for _ in 0..150 {
        cpu.tick(&mut bus);
    }

    // 100 / 7 = 14 remainder 2
    // Result: remainder(high) : quotient(low) = 0x0002_000E
    assert_eq!(cpu.regs.d[0], 0x0002_000E);
}

#[test]
fn test_divs() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // DIVS D1, D0 (opcode: 0x81C1)
    // 1000 000 111 000 001 = 0x81C1
    load_words(&mut bus, 0x1000, &[0x81C1]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0xFFFF_FF9C; // -100 as 32-bit signed
    cpu.regs.d[1] = 7;           // Divisor

    for _ in 0..170 {
        cpu.tick(&mut bus);
    }

    // -100 / 7 = -14 remainder -2
    // Result: remainder(high) : quotient(low)
    // -14 = 0xFFF2, -2 = 0xFFFE
    assert_eq!(cpu.regs.d[0], 0xFFFE_FFF2);
}

#[test]
fn test_mulu_memory() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MULU (A0), D0 (opcode: 0xC0D0)
    // 1100 000 011 010 000 = 0xC0D0
    // NOP to prevent garbage execution
    load_words(&mut bus, 0x1000, &[0xC0D0, 0x4E71]);
    load_words(&mut bus, 0x2000, &[0x0010]); // Source value: 16
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.a[0] = 0x2000;
    cpu.regs.d[0] = 0x0008; // Destination: 8

    for _ in 0..100 {
        cpu.tick(&mut bus);
    }

    // 8 * 16 = 128 = 0x80
    assert_eq!(cpu.regs.d[0], 0x0000_0080);
}

#[test]
fn test_divu_memory() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // DIVU (A0), D0 (opcode: 0x80D0)
    // 1000 000 011 010 000 = 0x80D0
    // NOP to prevent garbage execution
    load_words(&mut bus, 0x1000, &[0x80D0, 0x4E71]);
    load_words(&mut bus, 0x2000, &[0x0003]); // Divisor: 3
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.a[0] = 0x2000;
    cpu.regs.d[0] = 0x0000_000A; // Dividend: 10

    for _ in 0..200 {
        cpu.tick(&mut bus);
    }

    // 10 / 3 = 3 remainder 1
    // Result: remainder(high) : quotient(low) = 0x0001_0003
    assert_eq!(cpu.regs.d[0], 0x0001_0003);
}

#[test]
fn test_jmp_indirect() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // JMP (A0) (opcode: 0x4ED0)
    // 0100 1110 11 010 000 = 0x4ED0
    load_words(&mut bus, 0x1000, &[0x4ED0]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..12 {
        cpu.tick(&mut bus);
    }

    assert_eq!(cpu.regs.pc, 0x2000);
}

#[test]
fn test_jsr_indirect() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // JSR (A0) (opcode: 0x4E90)
    // 0100 1110 10 010 000 = 0x4E90
    load_words(&mut bus, 0x1000, &[0x4E90]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);
    cpu.regs.set_a(7, 0x8000); // Stack at 0x8000

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // PC should be at subroutine
    assert_eq!(cpu.regs.pc, 0x2000);
    // Stack should have return address pushed (SP decremented by 4)
    assert_eq!(cpu.regs.a(7), 0x7FFC);
}

#[test]
fn test_pea_indirect() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // PEA (A0) (opcode: 0x4850)
    // 0100 1000 01 010 000 = 0x4850
    load_words(&mut bus, 0x1000, &[0x4850]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x1234_5678);
    cpu.regs.set_a(7, 0x8000);

    for _ in 0..16 {
        cpu.tick(&mut bus);
    }

    // SP should be decremented by 4
    assert_eq!(cpu.regs.a(7), 0x7FFC);
    // Value pushed should be the effective address (A0 value)
    let hi = u16::from(bus.peek(0x7FFC)) << 8 | u16::from(bus.peek(0x7FFD));
    let lo = u16::from(bus.peek(0x7FFE)) << 8 | u16::from(bus.peek(0x7FFF));
    let pushed = u32::from(hi) << 16 | u32::from(lo);
    assert_eq!(pushed, 0x1234_5678);
}

#[test]
fn test_lea_indirect() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // LEA (A0), A1 (opcode: 0x43D0)
    // 0100 001 111 010 000 = 0x43D0
    load_words(&mut bus, 0x1000, &[0x43D0]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x1234_5678);

    run_instruction(&mut cpu, &mut bus);

    assert_eq!(cpu.regs.a(1), 0x1234_5678);
}

#[test]
fn test_lea_displacement() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // LEA $0010(A0), A1 (opcode: 0x43E8, displacement: 0x0010)
    // 0100 001 111 101 000 = 0x43E8
    load_words(&mut bus, 0x1000, &[0x43E8, 0x0010]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x0000_2000);

    for _ in 0..16 {
        cpu.tick(&mut bus);
    }

    assert_eq!(cpu.regs.a(1), 0x0000_2010); // 0x2000 + 0x10 = 0x2010
}

#[test]
fn test_lea_absolute_short() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // LEA $1234.W, A2 (opcode: 0x45F8, address: 0x1234)
    // 0100 010 111 111 000 = 0x45F8
    load_words(&mut bus, 0x1000, &[0x45F8, 0x1234]);
    cpu.reset();
    cpu.regs.pc = 0x1000;

    for _ in 0..16 {
        cpu.tick(&mut bus);
    }

    assert_eq!(cpu.regs.a(2), 0x0000_1234);
}

// === Word Displacement Branch Tests ===

#[test]
fn test_bra_word_displacement() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // BRA.W $0010 (opcode: 0x6000, displacement: 0x0010)
    // Branch forward 16 bytes from the start of the extension word
    load_words(&mut bus, 0x1000, &[0x6000, 0x0010]);
    cpu.reset();
    cpu.regs.pc = 0x1000;

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // PC should be at 0x1002 + 0x0010 = 0x1012
    assert_eq!(cpu.regs.pc, 0x1012);
}

#[test]
fn test_bra_word_backward() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // BRA.W $FFF0 (opcode: 0x6000, displacement: -16)
    // Place at 0x1020 to have room for backward branch
    load_words(&mut bus, 0x1020, &[0x6000, 0xFFF0]);
    cpu.reset();
    cpu.regs.pc = 0x1020;

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // PC should be at 0x1022 + (-16) = 0x1022 - 16 = 0x1012
    assert_eq!(cpu.regs.pc, 0x1012);
}

#[test]
fn test_beq_word_taken() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // BEQ.W $0020 (opcode: 0x6700, displacement: 0x0020)
    load_words(&mut bus, 0x1000, &[0x6700, 0x0020]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.sr |= emu_68000::Z; // Set Z flag so branch is taken

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // PC should be at 0x1002 + 0x0020 = 0x1022
    assert_eq!(cpu.regs.pc, 0x1022);
}

#[test]
fn test_beq_word_not_taken() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // BEQ.W $0020 (opcode: 0x6700, displacement: 0x0020)
    // Put a NOP at 0x1004 to have a clean boundary
    load_words(&mut bus, 0x1000, &[0x6700, 0x0020, 0x4E71]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.sr &= !emu_68000::Z; // Clear Z flag so branch is not taken

    for _ in 0..16 {
        cpu.tick(&mut bus);
    }

    // PC should skip past instruction and start fetching NOP: 0x1000 + 4 = 0x1004
    // After NOP fetch, PC will be 0x1006
    // Just verify we advanced past the BEQ.W (PC >= 0x1004)
    assert!(cpu.regs.pc >= 0x1004, "PC should be past BEQ.W instruction, got {:04X}", cpu.regs.pc);
}

// === MOVE USP Test ===

#[test]
fn test_move_usp_to_register() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MOVE USP,A3 (opcode: 0x4E6B)
    // 0100 1110 0110 1011 = 0x4E6B
    load_words(&mut bus, 0x1000, &[0x4E6B]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.sr |= 0x2000; // Set supervisor mode
    cpu.regs.usp = 0x0000_8000;

    run_instruction(&mut cpu, &mut bus);

    assert_eq!(cpu.regs.a(3), 0x0000_8000);
}

#[test]
fn test_move_register_to_usp() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MOVE A2,USP (opcode: 0x4E62)
    // 0100 1110 0110 0010 = 0x4E62
    load_words(&mut bus, 0x1000, &[0x4E62]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.sr |= 0x2000; // Set supervisor mode
    cpu.regs.set_a(2, 0x0000_6000);

    run_instruction(&mut cpu, &mut bus);

    assert_eq!(cpu.regs.usp, 0x0000_6000);
}

// === Immediate Operation Tests ===

#[test]
fn test_addi_word() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ADDI.W #$1234, D0 (opcode: 0x0640, immediate: 0x1234)
    // 0000 011 001 000 000 = 0x0640
    load_words(&mut bus, 0x1000, &[0x0640, 0x1234]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_1000;

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    assert_eq!(cpu.regs.d[0], 0x0000_2234); // 0x1000 + 0x1234 = 0x2234
}

#[test]
fn test_subi_long() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // SUBI.L #$00000100, D1 (opcode: 0x0481, immediate: 0x0000 0x0100)
    // 0000 010 010 000 001 = 0x0481
    load_words(&mut bus, 0x1000, &[0x0481, 0x0000, 0x0100]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[1] = 0x0000_0500;

    for _ in 0..24 {
        cpu.tick(&mut bus);
    }

    assert_eq!(cpu.regs.d[1], 0x0000_0400); // 0x500 - 0x100 = 0x400
}

#[test]
fn test_cmpi_word_sets_zero() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // CMPI.W #$5678, D2 (opcode: 0x0C42, immediate: 0x5678)
    // 0000 110 001 000 010 = 0x0C42
    load_words(&mut bus, 0x1000, &[0x0C42, 0x5678]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[2] = 0x0000_5678; // Same value as immediate

    for _ in 0..16 {
        cpu.tick(&mut bus);
    }

    // Z flag should be set because D2 - #$5678 = 0
    assert!(cpu.regs.sr & emu_68000::Z != 0, "Z flag should be set");
}

#[test]
fn test_andi_byte() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ANDI.B #$0F, D3 (opcode: 0x0203, immediate: 0x000F)
    // 0000 001 000 000 011 = 0x0203
    load_words(&mut bus, 0x1000, &[0x0203, 0x000F]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[3] = 0x1234_56AB;

    for _ in 0..16 {
        cpu.tick(&mut bus);
    }

    // Low byte should be 0xAB & 0x0F = 0x0B, rest unchanged
    assert_eq!(cpu.regs.d[3], 0x1234_560B);
}

#[test]
fn test_ori_word() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ORI.W #$FF00, D4 (opcode: 0x0044, immediate: 0xFF00)
    // 0000 000 001 000 100 = 0x0044
    load_words(&mut bus, 0x1000, &[0x0044, 0xFF00]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[4] = 0x0000_00FF;

    for _ in 0..16 {
        cpu.tick(&mut bus);
    }

    // Low word should be 0x00FF | 0xFF00 = 0xFFFF
    assert_eq!(cpu.regs.d[4], 0x0000_FFFF);
}

#[test]
fn test_eori_long() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // EORI.L #$FFFFFFFF, D5 (opcode: 0x0A85, immediate: 0xFFFF 0xFFFF)
    // 0000 101 010 000 101 = 0x0A85
    load_words(&mut bus, 0x1000, &[0x0A85, 0xFFFF, 0xFFFF]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[5] = 0x5555_AAAA;

    for _ in 0..24 {
        cpu.tick(&mut bus);
    }

    // XOR with all 1s inverts all bits
    assert_eq!(cpu.regs.d[5], 0xAAAA_5555);
}

// === Immediate Bit Operations ===

#[test]
fn test_btst_immediate() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // BTST #3, D0 (opcode: 0x0800, bit number: 0x0003)
    // 0000 1000 00 000 000 = 0x0800
    load_words(&mut bus, 0x1000, &[0x0800, 0x0003]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_0008; // Bit 3 is set

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Z flag should be clear because bit 3 was set
    assert!(cpu.regs.sr & emu_68000::Z == 0, "Z should be clear when bit is set");
}

#[test]
fn test_btst_immediate_zero() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // BTST #3, D0 when bit 3 is clear
    load_words(&mut bus, 0x1000, &[0x0800, 0x0003]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_0000; // Bit 3 is clear

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Z flag should be set because bit 3 was clear
    assert!(cpu.regs.sr & emu_68000::Z != 0, "Z should be set when bit is clear");
}

#[test]
fn test_bset_immediate() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // BSET #5, D1 (opcode: 0x08C1, bit number: 0x0005)
    // 0000 1000 11 000 001 = 0x08C1
    load_words(&mut bus, 0x1000, &[0x08C1, 0x0005]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[1] = 0x0000_0000;

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Bit 5 should now be set
    assert_eq!(cpu.regs.d[1], 0x0000_0020);
    // Z should be set (bit was originally clear)
    assert!(cpu.regs.sr & emu_68000::Z != 0);
}

#[test]
fn test_bclr_immediate() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // BCLR #4, D2 (opcode: 0x0882, bit number: 0x0004)
    // 0000 1000 10 000 010 = 0x0882
    load_words(&mut bus, 0x1000, &[0x0882, 0x0004]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[2] = 0x0000_00FF;

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Bit 4 should now be clear
    assert_eq!(cpu.regs.d[2], 0x0000_00EF);
    // Z should be clear (bit was originally set)
    assert!(cpu.regs.sr & emu_68000::Z == 0);
}

// === Bit Operations on Memory ===

#[test]
fn test_btst_reg_memory() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // BTST D0, (A0) - test bit in memory (mode=010, ea_reg=0)
    // 0000 rrr1 00 mmm rrr = 0000 0001 00 010 000 = 0x0110
    // NOP (0x4E71) added to prevent garbage execution
    load_words(&mut bus, 0x1000, &[0x0110, 0x4E71]);
    bus.poke(0x2000, 0xFB); // Bit 2 is clear
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 2; // Test bit 2
    cpu.regs.a[0] = 0x2000;

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Z should be set (bit 2 was clear)
    assert!(cpu.regs.sr & emu_68000::Z != 0);
    // Memory should be unchanged (BTST is read-only)
    assert_eq!(bus.peek(0x2000), 0xFB);
}

#[test]
fn test_bchg_reg_memory() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // BCHG D1, (A0) - toggle bit in memory
    // 0000 rrr1 01 mmm rrr = 0000 0011 01 010 000 = 0x0350
    // NOP (0x4E71) added to prevent garbage execution
    load_words(&mut bus, 0x1000, &[0x0350, 0x4E71]);
    bus.poke(0x2000, 0x55); // 0101_0101
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[1] = 3; // Toggle bit 3
    cpu.regs.a[0] = 0x2000;

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Z should be set (bit 3 was clear before)
    assert!(cpu.regs.sr & emu_68000::Z != 0);
    // Bit 3 should now be toggled: 0101_0101 -> 0101_1101 = 0x5D
    assert_eq!(bus.peek(0x2000), 0x5D);
}

#[test]
fn test_bclr_reg_memory() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // BCLR D2, (A0) - clear bit in memory
    // 0000 rrr1 10 mmm rrr = 0000 0101 10 010 000 = 0x0590
    // NOP (0x4E71) added to prevent garbage execution
    load_words(&mut bus, 0x1000, &[0x0590, 0x4E71]);
    bus.poke(0x2000, 0xFF);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[2] = 5; // Clear bit 5
    cpu.regs.a[0] = 0x2000;

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Z should be clear (bit 5 was set before)
    assert!(cpu.regs.sr & emu_68000::Z == 0);
    // Bit 5 should be cleared: 0xFF -> 0xDF
    assert_eq!(bus.peek(0x2000), 0xDF);
}

#[test]
fn test_bset_reg_memory() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // BSET D3, (A0) - set bit in memory
    // 0000 rrr1 11 mmm rrr = 0000 0111 11 010 000 = 0x07D0
    // NOP (0x4E71) added to prevent garbage execution
    load_words(&mut bus, 0x1000, &[0x07D0, 0x4E71]);
    bus.poke(0x2000, 0x00);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[3] = 7; // Set bit 7
    cpu.regs.a[0] = 0x2000;

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Z should be set (bit 7 was clear before)
    assert!(cpu.regs.sr & emu_68000::Z != 0);
    // Bit 7 should be set: 0x00 -> 0x80
    assert_eq!(bus.peek(0x2000), 0x80);
}

#[test]
fn test_bset_reg_memory_bit_mod8() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // BSET D0, (A0) - set bit in memory, bit number > 7 should mod 8
    // 0000 rrr1 11 mmm rrr = 0000 0001 11 010 000 = 0x01D0
    // NOP (0x4E71) added to prevent garbage execution
    load_words(&mut bus, 0x1000, &[0x01D0, 0x4E71]);
    bus.poke(0x2000, 0x00);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 10; // Bit 10 mod 8 = bit 2
    cpu.regs.a[0] = 0x2000;

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Z should be set (bit 2 was clear before)
    assert!(cpu.regs.sr & emu_68000::Z != 0);
    // Bit 2 should be set: 0x00 -> 0x04
    assert_eq!(bus.peek(0x2000), 0x04);
}

// === Status Register Operations ===

#[test]
fn test_move_from_sr() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MOVE SR, D0 (opcode: 0x40C0)
    // 0100 0000 11 000 000 = 0x40C0
    load_words(&mut bus, 0x1000, &[0x40C0]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.sr = 0x271F; // Supervisor + some flags
    cpu.regs.d[0] = 0xFFFF_FFFF;

    run_instruction(&mut cpu, &mut bus);

    // Low word of D0 should contain SR
    assert_eq!(cpu.regs.d[0] & 0xFFFF, 0x271F);
    // High word should be preserved
    assert_eq!(cpu.regs.d[0] & 0xFFFF_0000, 0xFFFF_0000);
}

#[test]
fn test_move_to_ccr_register() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MOVE D3, CCR (opcode: 0x44C3)
    // 0100 0100 11 000 011 = 0x44C3
    load_words(&mut bus, 0x1000, &[0x44C3]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.sr = 0x2700; // Supervisor, no flags
    cpu.regs.d[3] = 0x0000_001F; // All CCR flags set

    run_instruction(&mut cpu, &mut bus);

    // CCR (low 5 bits of SR) should be set
    assert_eq!(cpu.regs.sr & 0x1F, 0x1F);
    // System byte should be unchanged
    assert_eq!(cpu.regs.sr & 0xFF00, 0x2700);
}

#[test]
fn test_move_to_sr_privileged() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MOVE D4, SR (opcode: 0x46C4)
    // 0100 0110 11 000 100 = 0x46C4
    load_words(&mut bus, 0x1000, &[0x46C4]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.sr = 0x2700; // Supervisor mode
    cpu.regs.d[4] = 0x0000_2715;

    run_instruction(&mut cpu, &mut bus);

    // Entire SR should be updated
    assert_eq!(cpu.regs.sr, 0x2715);
}

#[test]
fn test_negx_with_extend() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // NEGX.L D0 (opcode: 0x4080)
    // 0100 0000 10 000 000 = 0x4080
    load_words(&mut bus, 0x1000, &[0x4080]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_0001;
    cpu.regs.sr |= emu_68000::X; // Set extend flag

    run_instruction(&mut cpu, &mut bus);

    // 0 - 1 - 1 = -2 = 0xFFFF_FFFE
    assert_eq!(cpu.regs.d[0], 0xFFFF_FFFE);
}

// === Scc (Set on Condition) ===

#[test]
fn test_seq_true() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // SEQ D0 (opcode: 0x57C0) - Set if equal
    // 0101 0111 11 000 000 = 0x57C0
    load_words(&mut bus, 0x1000, &[0x57C0]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.sr |= emu_68000::Z; // Set Z flag so condition is true
    cpu.regs.d[0] = 0x0000_0000;

    run_instruction(&mut cpu, &mut bus);

    // Low byte should be 0xFF
    assert_eq!(cpu.regs.d[0] & 0xFF, 0xFF);
}

#[test]
fn test_seq_false() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // SEQ D0 (opcode: 0x57C0) - Set if equal
    load_words(&mut bus, 0x1000, &[0x57C0]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.sr &= !emu_68000::Z; // Clear Z flag so condition is false
    cpu.regs.d[0] = 0x0000_00FF;

    run_instruction(&mut cpu, &mut bus);

    // Low byte should be 0x00
    assert_eq!(cpu.regs.d[0] & 0xFF, 0x00);
}

#[test]
fn test_sne() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // SNE D1 (opcode: 0x56C1) - Set if not equal
    // 0101 0110 11 000 001 = 0x56C1
    load_words(&mut bus, 0x1000, &[0x56C1]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.sr &= !emu_68000::Z; // Z clear means not equal
    cpu.regs.d[1] = 0x0000_0000;

    run_instruction(&mut cpu, &mut bus);

    // Low byte should be 0xFF (condition true)
    assert_eq!(cpu.regs.d[1] & 0xFF, 0xFF);
}

// === ADDX/SUBX (Multi-precision arithmetic) ===

#[test]
fn test_addx_with_extend() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ADDX.L D0,D1 (opcode: 0xD380)
    // 1101 001 1 10 00 0 000 = 0xD380
    load_words(&mut bus, 0x1000, &[0xD380]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_0001;
    cpu.regs.d[1] = 0x0000_0002;
    cpu.regs.sr |= emu_68000::X; // Set extend flag

    run_instruction(&mut cpu, &mut bus);

    // D1 = D1 + D0 + X = 2 + 1 + 1 = 4
    assert_eq!(cpu.regs.d[1], 0x0000_0004);
}

#[test]
fn test_subx_with_extend() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // SUBX.L D0,D1 (opcode: 0x9380)
    // 1001 001 1 10 00 0 000 = 0x9380
    load_words(&mut bus, 0x1000, &[0x9380]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_0001;
    cpu.regs.d[1] = 0x0000_0005;
    cpu.regs.sr |= emu_68000::X; // Set extend flag

    run_instruction(&mut cpu, &mut bus);

    // D1 = D1 - D0 - X = 5 - 1 - 1 = 3
    assert_eq!(cpu.regs.d[1], 0x0000_0003);
}

// === BCD Operations (ABCD, SBCD, NBCD) ===

#[test]
fn test_abcd_simple() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ABCD D0,D1 (opcode: 0xC300)
    // 1100 001 10000 0 000 = 0xC300
    load_words(&mut bus, 0x1000, &[0xC300]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_0025; // 25 BCD
    cpu.regs.d[1] = 0x0000_0034; // 34 BCD
    cpu.regs.sr &= !emu_68000::X; // Clear extend

    run_instruction(&mut cpu, &mut bus);

    // 25 + 34 = 59 BCD
    assert_eq!(cpu.regs.d[1] & 0xFF, 0x59);
    // No carry
    assert!(cpu.regs.sr & emu_68000::C == 0);
}

#[test]
fn test_abcd_with_carry() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ABCD D0,D1
    load_words(&mut bus, 0x1000, &[0xC300]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_0099; // 99 BCD
    cpu.regs.d[1] = 0x0000_0001; // 01 BCD
    cpu.regs.sr &= !emu_68000::X;

    run_instruction(&mut cpu, &mut bus);

    // 99 + 01 = 100 BCD, but only 00 fits in byte
    assert_eq!(cpu.regs.d[1] & 0xFF, 0x00);
    // Carry should be set
    assert!(cpu.regs.sr & emu_68000::C != 0);
    assert!(cpu.regs.sr & emu_68000::X != 0);
}

#[test]
fn test_abcd_with_extend() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ABCD D0,D1
    load_words(&mut bus, 0x1000, &[0xC300]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_0010; // 10 BCD
    cpu.regs.d[1] = 0x0000_0020; // 20 BCD
    cpu.regs.sr |= emu_68000::X; // Set extend

    run_instruction(&mut cpu, &mut bus);

    // 10 + 20 + 1 = 31 BCD
    assert_eq!(cpu.regs.d[1] & 0xFF, 0x31);
}

#[test]
fn test_abcd_low_nibble_correction() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ABCD D0,D1
    load_words(&mut bus, 0x1000, &[0xC300]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_0009; // 09 BCD
    cpu.regs.d[1] = 0x0000_0008; // 08 BCD
    cpu.regs.sr &= !emu_68000::X;

    run_instruction(&mut cpu, &mut bus);

    // 09 + 08 = 17 BCD (requires low nibble correction: 9+8=17, 17>9 so +6=23, take low nibble 7, carry 1)
    assert_eq!(cpu.regs.d[1] & 0xFF, 0x17);
}

#[test]
fn test_sbcd_simple() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // SBCD D0,D1 (opcode: 0x8300)
    // 1000 001 10000 0 000 = 0x8300
    load_words(&mut bus, 0x1000, &[0x8300]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_0025; // 25 BCD (subtrahend)
    cpu.regs.d[1] = 0x0000_0059; // 59 BCD (minuend)
    cpu.regs.sr &= !emu_68000::X;

    run_instruction(&mut cpu, &mut bus);

    // 59 - 25 = 34 BCD
    assert_eq!(cpu.regs.d[1] & 0xFF, 0x34);
    // No borrow
    assert!(cpu.regs.sr & emu_68000::C == 0);
}

#[test]
fn test_sbcd_with_borrow() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // SBCD D0,D1
    load_words(&mut bus, 0x1000, &[0x8300]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_0050; // 50 BCD
    cpu.regs.d[1] = 0x0000_0025; // 25 BCD
    cpu.regs.sr &= !emu_68000::X;

    run_instruction(&mut cpu, &mut bus);

    // 25 - 50 = -25, which wraps to 75 BCD with borrow
    assert_eq!(cpu.regs.d[1] & 0xFF, 0x75);
    // Borrow should be set
    assert!(cpu.regs.sr & emu_68000::C != 0);
    assert!(cpu.regs.sr & emu_68000::X != 0);
}

#[test]
fn test_sbcd_with_extend() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // SBCD D0,D1
    load_words(&mut bus, 0x1000, &[0x8300]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_0010; // 10 BCD
    cpu.regs.d[1] = 0x0000_0032; // 32 BCD
    cpu.regs.sr |= emu_68000::X; // Set extend

    run_instruction(&mut cpu, &mut bus);

    // 32 - 10 - 1 = 21 BCD
    assert_eq!(cpu.regs.d[1] & 0xFF, 0x21);
}

#[test]
fn test_nbcd_simple() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // NBCD D0 (opcode: 0x4800)
    // 0100 1000 00 000 000 = 0x4800
    load_words(&mut bus, 0x1000, &[0x4800]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_0025; // 25 BCD
    cpu.regs.sr &= !emu_68000::X;
    cpu.regs.sr |= emu_68000::Z; // Set Z to verify it gets cleared

    run_instruction(&mut cpu, &mut bus);

    // 0 - 25 = 75 BCD (100's complement)
    assert_eq!(cpu.regs.d[0] & 0xFF, 0x75);
    // Borrow should be set
    assert!(cpu.regs.sr & emu_68000::C != 0);
    // Z should be cleared (result non-zero)
    assert!(cpu.regs.sr & emu_68000::Z == 0);
}

#[test]
fn test_nbcd_zero() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // NBCD D0
    load_words(&mut bus, 0x1000, &[0x4800]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_0000; // 00 BCD
    cpu.regs.sr &= !emu_68000::X;
    cpu.regs.sr |= emu_68000::Z; // Set Z

    run_instruction(&mut cpu, &mut bus);

    // 0 - 0 = 0 BCD
    assert_eq!(cpu.regs.d[0] & 0xFF, 0x00);
    // No borrow
    assert!(cpu.regs.sr & emu_68000::C == 0);
    // Z should remain unchanged (was set, result is zero)
    assert!(cpu.regs.sr & emu_68000::Z != 0);
}

#[test]
fn test_nbcd_with_extend() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // NBCD D0
    load_words(&mut bus, 0x1000, &[0x4800]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_0000; // 00 BCD
    cpu.regs.sr |= emu_68000::X; // Set extend

    run_instruction(&mut cpu, &mut bus);

    // 0 - 0 - 1 = 99 BCD (with borrow)
    assert_eq!(cpu.regs.d[0] & 0xFF, 0x99);
    // Borrow should be set
    assert!(cpu.regs.sr & emu_68000::C != 0);
}

// === CMPM (Compare Memory) ===

#[test]
fn test_cmpm_byte_equal() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // CMPM.B (A0)+,(A1)+ (opcode: 0xB308)
    // 1011 001 1 00 001 000 = 0xB308
    load_words(&mut bus, 0x1000, &[0xB308]);
    // Put equal bytes at source and destination
    bus.poke(0x2000, 0x42); // Source (A0)
    bus.poke(0x3000, 0x42); // Destination (A1)

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000); // Source
    cpu.regs.set_a(1, 0x3000); // Destination

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Z flag should be set (equal)
    assert!(cpu.regs.sr & emu_68000::Z != 0, "Z should be set for equal values");
    // Both address registers should be incremented by 1
    assert_eq!(cpu.regs.a(0), 0x2001, "A0 should be incremented");
    assert_eq!(cpu.regs.a(1), 0x3001, "A1 should be incremented");
}

#[test]
fn test_cmpm_byte_not_equal() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // CMPM.B (A0)+,(A1)+
    load_words(&mut bus, 0x1000, &[0xB308]);
    bus.poke(0x2000, 0x10); // Source (A0)
    bus.poke(0x3000, 0x20); // Destination (A1)

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);
    cpu.regs.set_a(1, 0x3000);

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Z flag should be clear (not equal)
    assert!(cpu.regs.sr & emu_68000::Z == 0, "Z should be clear for unequal values");
    // N flag should be clear (0x20 - 0x10 = 0x10, positive)
    assert!(cpu.regs.sr & emu_68000::N == 0, "N should be clear");
}

#[test]
fn test_cmpm_byte_negative_result() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // CMPM.B (A0)+,(A1)+
    load_words(&mut bus, 0x1000, &[0xB308]);
    bus.poke(0x2000, 0x20); // Source (A0)
    bus.poke(0x3000, 0x10); // Destination (A1) - smaller

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);
    cpu.regs.set_a(1, 0x3000);

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // 0x10 - 0x20 = 0xF0 (negative, with borrow)
    assert!(cpu.regs.sr & emu_68000::N != 0, "N should be set for negative result");
    assert!(cpu.regs.sr & emu_68000::C != 0, "C should be set for borrow");
}

#[test]
fn test_cmpm_word() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // CMPM.W (A0)+,(A1)+ (opcode: 0xB348)
    // 1011 001 1 01 001 000 = 0xB348
    load_words(&mut bus, 0x1000, &[0xB348]);
    load_words(&mut bus, 0x2000, &[0x1234]); // Source
    load_words(&mut bus, 0x3000, &[0x1234]); // Destination (equal)

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);
    cpu.regs.set_a(1, 0x3000);

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Z flag should be set
    assert!(cpu.regs.sr & emu_68000::Z != 0);
    // Address registers incremented by 2
    assert_eq!(cpu.regs.a(0), 0x2002);
    assert_eq!(cpu.regs.a(1), 0x3002);
}

#[test]
fn test_cmpm_long() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // CMPM.L (A0)+,(A1)+ (opcode: 0xB388)
    // 1011 001 1 10 001 000 = 0xB388
    load_words(&mut bus, 0x1000, &[0xB388]);
    load_words(&mut bus, 0x2000, &[0xDEAD, 0xBEEF]); // Source
    load_words(&mut bus, 0x3000, &[0xDEAD, 0xBEEF]); // Destination (equal)

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);
    cpu.regs.set_a(1, 0x3000);

    for _ in 0..30 {
        cpu.tick(&mut bus);
    }

    // Z flag should be set
    assert!(cpu.regs.sr & emu_68000::Z != 0);
    // Address registers incremented by 4
    assert_eq!(cpu.regs.a(0), 0x2004);
    assert_eq!(cpu.regs.a(1), 0x3004);
}

#[test]
fn test_cmpm_a7_byte_increment() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // CMPM.B (A7)+,(A0)+ - A7 should increment by 2 for byte ops
    // 1011 000 1 00 001 111 = 0xB10F
    load_words(&mut bus, 0x1000, &[0xB10F]);
    bus.poke(0x4000, 0x55); // Source at A7
    bus.poke(0x2000, 0x55); // Destination at A0

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(7, 0x4000);
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // A7 should increment by 2 (stack pointer alignment)
    assert_eq!(cpu.regs.a(7), 0x4002, "A7 should increment by 2 for byte");
    // A0 should increment by 1
    assert_eq!(cpu.regs.a(0), 0x2001, "A0 should increment by 1 for byte");
}

// === MOVEM (Move Multiple Registers) ===

#[test]
fn test_movem_to_mem_word_indirect() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MOVEM.W D0/D1/D2,(A0) (opcode: 0x4890, mask: 0x0007)
    // 0100 1000 10 010 000 = 0x4890
    // Mask: bits 0,1,2 = D0,D1,D2
    load_words(&mut bus, 0x1000, &[0x4890, 0x0007]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);
    cpu.regs.d[0] = 0x1111_1111;
    cpu.regs.d[1] = 0x2222_2222;
    cpu.regs.d[2] = 0x3333_3333;

    for _ in 0..40 {
        cpu.tick(&mut bus);
    }

    // Check memory - words written at 0x2000, 0x2002, 0x2004
    let w0 = u16::from(bus.peek(0x2000)) << 8 | u16::from(bus.peek(0x2001));
    let w1 = u16::from(bus.peek(0x2002)) << 8 | u16::from(bus.peek(0x2003));
    let w2 = u16::from(bus.peek(0x2004)) << 8 | u16::from(bus.peek(0x2005));
    assert_eq!(w0, 0x1111, "D0 word at 0x2000");
    assert_eq!(w1, 0x2222, "D1 word at 0x2002");
    assert_eq!(w2, 0x3333, "D2 word at 0x2004");
}

#[test]
fn test_movem_to_mem_long_indirect() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MOVEM.L D0/D1,(A0) (opcode: 0x48D0, mask: 0x0003)
    // 0100 1000 11 010 000 = 0x48D0
    load_words(&mut bus, 0x1000, &[0x48D0, 0x0003]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);
    cpu.regs.d[0] = 0x1234_5678;
    cpu.regs.d[1] = 0xABCD_EF01;

    for _ in 0..50 {
        cpu.tick(&mut bus);
    }

    // Check memory - longs written at 0x2000 and 0x2004
    let read_long = |bus: &SimpleBus, addr: u16| {
        let hi = u16::from(bus.peek(addr)) << 8 | u16::from(bus.peek(addr + 1));
        let lo = u16::from(bus.peek(addr + 2)) << 8 | u16::from(bus.peek(addr + 3));
        u32::from(hi) << 16 | u32::from(lo)
    };
    assert_eq!(read_long(&bus, 0x2000), 0x1234_5678, "D0 at 0x2000");
    assert_eq!(read_long(&bus, 0x2004), 0xABCD_EF01, "D1 at 0x2004");
}

#[test]
fn test_movem_to_mem_predec() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MOVEM.L D0/D1,-(A0) (opcode: 0x48E0, mask: 0x0003)
    // 0100 1000 11 100 000 = 0x48E0
    // For predecrement, mask is reversed: bit 0=A7, bit 15=D0
    // To store D0/D1: bits 14,15 = 0xC000
    load_words(&mut bus, 0x1000, &[0x48E0, 0xC000]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2010); // Start high, decrement down
    cpu.regs.d[0] = 0x1111_1111;
    cpu.regs.d[1] = 0x2222_2222;

    for _ in 0..50 {
        cpu.tick(&mut bus);
    }

    // A0 should be decremented by 8 (2 longs)
    assert_eq!(cpu.regs.a(0), 0x2008, "A0 should be 0x2008 after predec");

    // For predecrement: D0 is written first (highest bit in reversed mask)
    // Writes happen at decremented addresses
    let read_long = |bus: &SimpleBus, addr: u16| {
        let hi = u16::from(bus.peek(addr)) << 8 | u16::from(bus.peek(addr + 1));
        let lo = u16::from(bus.peek(addr + 2)) << 8 | u16::from(bus.peek(addr + 3));
        u32::from(hi) << 16 | u32::from(lo)
    };
    assert_eq!(read_long(&bus, 0x2008), 0x1111_1111, "D0 at 0x2008");
    assert_eq!(read_long(&bus, 0x200C), 0x2222_2222, "D1 at 0x200C");
}

#[test]
fn test_movem_from_mem_word_indirect() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MOVEM.W (A0),D0/D1/D2 (opcode: 0x4C90, mask: 0x0007)
    // 0100 1100 10 010 000 = 0x4C90
    load_words(&mut bus, 0x1000, &[0x4C90, 0x0007]);
    // Data at 0x2000
    load_words(&mut bus, 0x2000, &[0x1111, 0x2222, 0x3333]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);
    cpu.regs.d[0] = 0xFFFF_FFFF;
    cpu.regs.d[1] = 0xFFFF_FFFF;
    cpu.regs.d[2] = 0xFFFF_FFFF;

    for _ in 0..40 {
        cpu.tick(&mut bus);
    }

    // Word loads to data registers don't sign extend, just load low word
    assert_eq!(cpu.regs.d[0] & 0xFFFF, 0x1111);
    assert_eq!(cpu.regs.d[1] & 0xFFFF, 0x2222);
    assert_eq!(cpu.regs.d[2] & 0xFFFF, 0x3333);
}

#[test]
fn test_movem_from_mem_long_indirect() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MOVEM.L (A0),D0/D1 (opcode: 0x4CD0, mask: 0x0003)
    // 0100 1100 11 010 000 = 0x4CD0
    load_words(&mut bus, 0x1000, &[0x4CD0, 0x0003]);
    // Data at 0x2000
    load_words(&mut bus, 0x2000, &[0x1234, 0x5678, 0xABCD, 0xEF01]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..50 {
        cpu.tick(&mut bus);
    }

    assert_eq!(cpu.regs.d[0], 0x1234_5678);
    assert_eq!(cpu.regs.d[1], 0xABCD_EF01);
}

#[test]
fn test_movem_from_mem_postinc() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MOVEM.L (A0)+,D0/D1 (opcode: 0x4CD8, mask: 0x0003)
    // 0100 1100 11 011 000 = 0x4CD8
    load_words(&mut bus, 0x1000, &[0x4CD8, 0x0003]);
    // Data at 0x2000
    load_words(&mut bus, 0x2000, &[0xCAFE, 0xBABE, 0xDEAD, 0xBEEF]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..50 {
        cpu.tick(&mut bus);
    }

    assert_eq!(cpu.regs.d[0], 0xCAFE_BABE);
    assert_eq!(cpu.regs.d[1], 0xDEAD_BEEF);
    // A0 should be incremented by 8 (2 longs)
    assert_eq!(cpu.regs.a(0), 0x2008, "A0 should be 0x2008 after postinc");
}

#[test]
fn test_movem_from_mem_word_sign_extend_address_reg() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MOVEM.W (A0),A1 (opcode: 0x4C90, mask: 0x0200)
    // Mask bit 9 = A1
    load_words(&mut bus, 0x1000, &[0x4C90, 0x0200]);
    // Data at 0x2000 - 0xFFFF is -1 as signed word
    load_words(&mut bus, 0x2000, &[0xFFFF]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);
    cpu.regs.set_a(1, 0x0000_0000);

    for _ in 0..30 {
        cpu.tick(&mut bus);
    }

    // Word to address register should sign-extend to 32 bits
    assert_eq!(cpu.regs.a(1), 0xFFFF_FFFF, "A1 should be sign-extended");
}

#[test]
fn test_movem_empty_mask() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MOVEM.L (A0)+,<nothing> (opcode: 0x4CD8, mask: 0x0000)
    load_words(&mut bus, 0x1000, &[0x4CD8, 0x0000]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // A0 should be unchanged (no registers transferred)
    assert_eq!(cpu.regs.a(0), 0x2000, "A0 unchanged with empty mask");
}

#[test]
fn test_movem_all_data_registers() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MOVEM.W D0-D7,(A0) (opcode: 0x4890, mask: 0x00FF)
    load_words(&mut bus, 0x1000, &[0x4890, 0x00FF]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);
    for i in 0..8 {
        cpu.regs.d[i] = (i as u32 + 1) * 0x1111_1111;
    }

    for _ in 0..80 {
        cpu.tick(&mut bus);
    }

    // Check all 8 words in memory
    for i in 0..8 {
        let addr = 0x2000 + (i * 2) as u16;
        let w = u16::from(bus.peek(addr)) << 8 | u16::from(bus.peek(addr + 1));
        let expected = ((i as u16 + 1) * 0x1111) as u16;
        assert_eq!(w, expected, "D{} word at {:04X}", i, addr);
    }
}

// ============================================================================
// TAS (Test And Set) Tests
// ============================================================================

#[test]
fn test_tas_data_register() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // TAS D0 (opcode: 0x4AC0), followed by NOP (0x4E71)
    load_words(&mut bus, 0x1000, &[0x4AC0, 0x4E71]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0000_0055; // Bit 7 clear

    for _ in 0..12 {
        cpu.tick(&mut bus);
    }

    // D0 low byte should have bit 7 set: 0x55 | 0x80 = 0xD5
    assert_eq!(cpu.regs.d[0], 0x0000_00D5);
    // Flags: N=0 (original was positive), Z=0 (original was non-zero)
    assert!(cpu.regs.sr & emu_68000::N == 0, "N flag should be clear for value 0x55, sr={:04X}", cpu.regs.sr);
    assert!(cpu.regs.sr & emu_68000::Z == 0);
}

#[test]
fn test_tas_data_register_zero() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // TAS D1 (opcode: 0x4AC1), followed by NOP
    load_words(&mut bus, 0x1000, &[0x4AC1, 0x4E71]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[1] = 0xFFFF_FF00; // Low byte is zero

    for _ in 0..12 {
        cpu.tick(&mut bus);
    }

    // D1 low byte should have bit 7 set: 0x00 | 0x80 = 0x80
    assert_eq!(cpu.regs.d[1], 0xFFFF_FF80);
    // Flags: Z=1 (original was zero), N=0
    assert!(cpu.regs.sr & emu_68000::Z != 0);
    assert!(cpu.regs.sr & emu_68000::N == 0);
}

#[test]
fn test_tas_data_register_negative() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // TAS D2 (opcode: 0x4AC2), followed by NOP
    load_words(&mut bus, 0x1000, &[0x4AC2, 0x4E71]);
    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[2] = 0x0000_0080; // Already negative (bit 7 set)

    for _ in 0..12 {
        cpu.tick(&mut bus);
    }

    // D2 unchanged (0x80 | 0x80 = 0x80)
    assert_eq!(cpu.regs.d[2], 0x0000_0080);
    // Flags: N=1 (original was negative), Z=0
    assert!(cpu.regs.sr & emu_68000::N != 0);
    assert!(cpu.regs.sr & emu_68000::Z == 0);
}

#[test]
fn test_tas_memory() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // TAS (A0) (opcode: 0x4AD0), followed by NOP
    load_words(&mut bus, 0x1000, &[0x4AD0, 0x4E71]);
    bus.poke(0x2000, 0x42); // Memory byte = 0x42

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Memory should have bit 7 set: 0x42 | 0x80 = 0xC2
    assert_eq!(bus.peek(0x2000), 0xC2);
    // Flags: N=0 (original was positive), Z=0
    assert!(cpu.regs.sr & emu_68000::N == 0);
    assert!(cpu.regs.sr & emu_68000::Z == 0);
}

// ============================================================================
// Memory Shift/Rotate Tests
// ============================================================================

#[test]
fn test_asl_memory() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ASL (A0) followed by NOP
    load_words(&mut bus, 0x1000, &[0xE1D0, 0x4E71]);
    // Memory word = 0x4000 (will shift to 0x8000, carry out 0)
    bus.poke(0x2000, 0x40);
    bus.poke(0x2001, 0x00);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..16 {
        cpu.tick(&mut bus);
    }

    // Result: 0x4000 << 1 = 0x8000
    let result = u16::from(bus.peek(0x2000)) << 8 | u16::from(bus.peek(0x2001));
    assert_eq!(result, 0x8000);
    // N=1 (result is negative), C=0 (bit 15 was 0)
    assert!(cpu.regs.sr & emu_68000::N != 0, "N should be set for result 0x8000");
    assert!(cpu.regs.sr & emu_68000::C == 0);
}

#[test]
fn test_asr_memory() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ASR (A0) followed by NOP
    load_words(&mut bus, 0x1000, &[0xE0D0, 0x4E71]);
    // Memory word = 0x8002 (negative, will shift to 0xC001, carry out 0)
    bus.poke(0x2000, 0x80);
    bus.poke(0x2001, 0x02);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..16 {
        cpu.tick(&mut bus);
    }

    // Result: 0x8002 >> 1 with sign extend = 0xC001
    let result = u16::from(bus.peek(0x2000)) << 8 | u16::from(bus.peek(0x2001));
    assert_eq!(result, 0xC001);
    // N=1 (result still negative), C=0 (bit 0 was 0)
    assert!(cpu.regs.sr & emu_68000::N != 0);
    assert!(cpu.regs.sr & emu_68000::C == 0);
}

#[test]
fn test_lsl_memory() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // LSL (A0) followed by NOP
    load_words(&mut bus, 0x1000, &[0xE3D0, 0x4E71]);
    // Memory word = 0x8001 (will shift to 0x0002, carry out 1)
    bus.poke(0x2000, 0x80);
    bus.poke(0x2001, 0x01);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..16 {
        cpu.tick(&mut bus);
    }

    // Result: 0x8001 << 1 = 0x0002
    let result = u16::from(bus.peek(0x2000)) << 8 | u16::from(bus.peek(0x2001));
    assert_eq!(result, 0x0002);
    // N=0, Z=0, C=1 (bit 15 was 1), X=1
    assert!(cpu.regs.sr & emu_68000::N == 0);
    assert!(cpu.regs.sr & emu_68000::Z == 0);
    assert!(cpu.regs.sr & emu_68000::C != 0);
    assert!(cpu.regs.sr & emu_68000::X != 0);
}

#[test]
fn test_lsr_memory() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // LSR (A0) followed by NOP
    load_words(&mut bus, 0x1000, &[0xE2D0, 0x4E71]);
    // Memory word = 0x0003 (will shift to 0x0001, carry out 1)
    bus.poke(0x2000, 0x00);
    bus.poke(0x2001, 0x03);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..16 {
        cpu.tick(&mut bus);
    }

    // Result: 0x0003 >> 1 = 0x0001
    let result = u16::from(bus.peek(0x2000)) << 8 | u16::from(bus.peek(0x2001));
    assert_eq!(result, 0x0001);
    // C=1 (bit 0 was 1), X=1
    assert!(cpu.regs.sr & emu_68000::C != 0);
    assert!(cpu.regs.sr & emu_68000::X != 0);
}

#[test]
fn test_rol_memory() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ROL (A0) followed by NOP
    load_words(&mut bus, 0x1000, &[0xE7D0, 0x4E71]);
    // Memory word = 0x8001 (will rotate to 0x0003)
    bus.poke(0x2000, 0x80);
    bus.poke(0x2001, 0x01);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..16 {
        cpu.tick(&mut bus);
    }

    // Result: rotate 0x8001 left = 0x0003
    let result = u16::from(bus.peek(0x2000)) << 8 | u16::from(bus.peek(0x2001));
    assert_eq!(result, 0x0003);
    // C=1 (bit 15 was 1), X unchanged for rotates
    assert!(cpu.regs.sr & emu_68000::C != 0);
}

#[test]
fn test_ror_memory() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ROR (A0) followed by NOP
    load_words(&mut bus, 0x1000, &[0xE6D0, 0x4E71]);
    // Memory word = 0x0003 (will rotate to 0x8001)
    bus.poke(0x2000, 0x00);
    bus.poke(0x2001, 0x03);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..16 {
        cpu.tick(&mut bus);
    }

    // Result: rotate 0x0003 right = 0x8001
    let result = u16::from(bus.peek(0x2000)) << 8 | u16::from(bus.peek(0x2001));
    assert_eq!(result, 0x8001);
    // C=1 (bit 0 was 1)
    assert!(cpu.regs.sr & emu_68000::C != 0);
}

// =============================================================================
// ALU Memory Destination Tests
// =============================================================================

#[test]
fn test_add_memory_word() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ADD.W D0,(A0) followed by NOP
    // Opcode: 1101 reg 1 01 mode reg = 1101 000 1 01 010 000 = 0xD150
    load_words(&mut bus, 0x1000, &[0xD150, 0x4E71]);
    // Memory word = 0x0010
    bus.poke(0x2000, 0x00);
    bus.poke(0x2001, 0x10);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0005;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Result: 0x0010 + 0x0005 = 0x0015
    let result = u16::from(bus.peek(0x2000)) << 8 | u16::from(bus.peek(0x2001));
    assert_eq!(result, 0x0015);
    // Z=0 (non-zero), N=0 (positive)
    assert!(cpu.regs.sr & emu_68000::Z == 0);
    assert!(cpu.regs.sr & emu_68000::N == 0);
}

#[test]
fn test_sub_memory_byte() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // SUB.B D0,(A0) followed by NOP
    // Opcode: 1001 reg 1 00 mode reg = 1001 000 1 00 010 000 = 0x9110
    load_words(&mut bus, 0x1000, &[0x9110, 0x4E71]);
    // Memory byte = 0x20
    bus.poke(0x2000, 0x20);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x05;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Result: 0x20 - 0x05 = 0x1B
    let result = bus.peek(0x2000);
    assert_eq!(result, 0x1B);
}

#[test]
fn test_and_memory_word() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // AND.W D0,(A0) followed by NOP
    // Opcode: 1100 reg 1 01 mode reg = 1100 000 1 01 010 000 = 0xC150
    load_words(&mut bus, 0x1000, &[0xC150, 0x4E71]);
    // Memory word = 0xFF0F
    bus.poke(0x2000, 0xFF);
    bus.poke(0x2001, 0x0F);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0F0F;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Result: 0xFF0F & 0x0F0F = 0x0F0F
    let result = u16::from(bus.peek(0x2000)) << 8 | u16::from(bus.peek(0x2001));
    assert_eq!(result, 0x0F0F);
}

#[test]
fn test_or_memory_byte() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // OR.B D0,(A0) followed by NOP
    // Opcode: 1000 reg 1 00 mode reg = 1000 000 1 00 010 000 = 0x8110
    load_words(&mut bus, 0x1000, &[0x8110, 0x4E71]);
    // Memory byte = 0x0F
    bus.poke(0x2000, 0x0F);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0xF0;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Result: 0x0F | 0xF0 = 0xFF
    let result = bus.peek(0x2000);
    assert_eq!(result, 0xFF);
    // N=1 (negative in byte), Z=0
    assert!(cpu.regs.sr & emu_68000::N != 0);
    assert!(cpu.regs.sr & emu_68000::Z == 0);
}

#[test]
fn test_eor_memory_word() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // EOR.W D0,(A0) followed by NOP
    // Opcode: 1011 reg 1 01 mode reg = 1011 000 1 01 010 000 = 0xB150
    load_words(&mut bus, 0x1000, &[0xB150, 0x4E71]);
    // Memory word = 0xAAAA
    bus.poke(0x2000, 0xAA);
    bus.poke(0x2001, 0xAA);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x5555;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Result: 0xAAAA ^ 0x5555 = 0xFFFF
    let result = u16::from(bus.peek(0x2000)) << 8 | u16::from(bus.peek(0x2001));
    assert_eq!(result, 0xFFFF);
    // N=1 (negative), Z=0
    assert!(cpu.regs.sr & emu_68000::N != 0);
    assert!(cpu.regs.sr & emu_68000::Z == 0);
}

#[test]
fn test_add_memory_with_carry() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ADD.W D0,(A0) followed by NOP
    load_words(&mut bus, 0x1000, &[0xD150, 0x4E71]);
    // Memory word = 0xFFFF
    bus.poke(0x2000, 0xFF);
    bus.poke(0x2001, 0xFF);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0001;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Result: 0xFFFF + 0x0001 = 0x0000 with carry
    let result = u16::from(bus.peek(0x2000)) << 8 | u16::from(bus.peek(0x2001));
    assert_eq!(result, 0x0000);
    // Z=1 (zero), C=1 (carry), X=1 (extend)
    assert!(cpu.regs.sr & emu_68000::Z != 0);
    assert!(cpu.regs.sr & emu_68000::C != 0);
    assert!(cpu.regs.sr & emu_68000::X != 0);
}

// =============================================================================
// ALU Memory Source Tests
// =============================================================================

#[test]
fn test_add_from_memory_word() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ADD.W (A0),D0 followed by NOP
    // Opcode: 1101 reg 0 size mode reg = 1101 000 0 01 010 000 = 0xD050
    load_words(&mut bus, 0x1000, &[0xD050, 0x4E71]);
    // Memory word = 0x0010
    bus.poke(0x2000, 0x00);
    bus.poke(0x2001, 0x10);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0005;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Result: D0 = 0x0005 + 0x0010 = 0x0015
    assert_eq!(cpu.regs.d[0] & 0xFFFF, 0x0015);
}

#[test]
fn test_sub_from_memory_byte() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // SUB.B (A0),D0 followed by NOP
    // Opcode: 1001 reg 0 size mode reg = 1001 000 0 00 010 000 = 0x9010
    load_words(&mut bus, 0x1000, &[0x9010, 0x4E71]);
    // Memory byte = 0x05
    bus.poke(0x2000, 0x05);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x20;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Result: D0 = 0x20 - 0x05 = 0x1B
    assert_eq!(cpu.regs.d[0] & 0xFF, 0x1B);
}

#[test]
fn test_and_from_memory_word() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // AND.W (A0),D0 followed by NOP
    // Opcode: 1100 reg 0 size mode reg = 1100 000 0 01 010 000 = 0xC050
    load_words(&mut bus, 0x1000, &[0xC050, 0x4E71]);
    // Memory word = 0x0F0F
    bus.poke(0x2000, 0x0F);
    bus.poke(0x2001, 0x0F);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0xFF00;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Result: D0 = 0xFF00 & 0x0F0F = 0x0F00
    assert_eq!(cpu.regs.d[0] & 0xFFFF, 0x0F00);
}

#[test]
fn test_or_from_memory_byte() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // OR.B (A0),D0 followed by NOP
    // Opcode: 1000 reg 0 size mode reg = 1000 000 0 00 010 000 = 0x8010
    load_words(&mut bus, 0x1000, &[0x8010, 0x4E71]);
    // Memory byte = 0x0F
    bus.poke(0x2000, 0x0F);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0xF0;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Result: D0 = 0xF0 | 0x0F = 0xFF
    assert_eq!(cpu.regs.d[0] & 0xFF, 0xFF);
}

#[test]
fn test_cmp_from_memory_word() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // CMP.W (A0),D0 followed by NOP
    // Opcode: 1011 reg 0 size mode reg = 1011 000 0 01 010 000 = 0xB050
    load_words(&mut bus, 0x1000, &[0xB050, 0x4E71]);
    // Memory word = 0x0010
    bus.poke(0x2000, 0x00);
    bus.poke(0x2001, 0x10);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x0010; // Equal values
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // D0 unchanged (CMP doesn't modify destination)
    assert_eq!(cpu.regs.d[0] & 0xFFFF, 0x0010);
    // Z=1 (equal)
    assert!(cpu.regs.sr & emu_68000::Z != 0);
}

#[test]
fn test_adda_from_memory_word() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ADDA.W (A1),A0 followed by NOP
    // Opcode: 1101 reg 011 mode reg = 1101 000 011 010 001 = 0xD0D1
    load_words(&mut bus, 0x1000, &[0xD0D1, 0x4E71]);
    // Memory word = 0xFFF0 (will sign-extend to 0xFFFFFFF0 = -16)
    bus.poke(0x2000, 0xFF);
    bus.poke(0x2001, 0xF0);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x0100);
    cpu.regs.set_a(1, 0x2000);

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Result: A0 = 0x0100 + 0xFFFFFFF0 = 0x000000F0
    assert_eq!(cpu.regs.a(0), 0x000000F0);
}

#[test]
fn test_add_from_memory_long() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ADD.L (A0),D0 followed by NOP
    // Opcode: 1101 reg 0 10 mode reg = 1101 000 0 10 010 000 = 0xD090
    load_words(&mut bus, 0x1000, &[0xD090, 0x4E71]);
    // Memory long = 0x12345678
    bus.poke(0x2000, 0x12);
    bus.poke(0x2001, 0x34);
    bus.poke(0x2002, 0x56);
    bus.poke(0x2003, 0x78);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x00000001;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..24 {
        cpu.tick(&mut bus);
    }

    // Result: D0 = 0x00000001 + 0x12345678 = 0x12345679
    assert_eq!(cpu.regs.d[0], 0x12345679);
}

// =============================================================================
// CLR/NEG/NOT/TST Memory Tests
// =============================================================================

#[test]
fn test_clr_memory_word() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // CLR.W (A0) followed by NOP
    // Opcode: 0100 0010 01 mode reg = 0100 0010 01 010 000 = 0x4250
    load_words(&mut bus, 0x1000, &[0x4250, 0x4E71]);
    // Memory word = 0x1234
    bus.poke(0x2000, 0x12);
    bus.poke(0x2001, 0x34);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Result: memory cleared to 0x0000
    let result = u16::from(bus.peek(0x2000)) << 8 | u16::from(bus.peek(0x2001));
    assert_eq!(result, 0x0000);
    // Z=1, N=0
    assert!(cpu.regs.sr & emu_68000::Z != 0);
    assert!(cpu.regs.sr & emu_68000::N == 0);
}

#[test]
fn test_neg_memory_word() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // NEG.W (A0) followed by NOP
    // Opcode: 0100 0100 01 mode reg = 0100 0100 01 010 000 = 0x4450
    load_words(&mut bus, 0x1000, &[0x4450, 0x4E71]);
    // Memory word = 0x0001
    bus.poke(0x2000, 0x00);
    bus.poke(0x2001, 0x01);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Result: 0 - 1 = 0xFFFF
    let result = u16::from(bus.peek(0x2000)) << 8 | u16::from(bus.peek(0x2001));
    assert_eq!(result, 0xFFFF);
    // N=1 (negative), C=1 (borrow), X=1
    assert!(cpu.regs.sr & emu_68000::N != 0);
    assert!(cpu.regs.sr & emu_68000::C != 0);
}

#[test]
fn test_not_memory_byte() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // NOT.B (A0) followed by NOP
    // Opcode: 0100 0110 00 mode reg = 0100 0110 00 010 000 = 0x4610
    load_words(&mut bus, 0x1000, &[0x4610, 0x4E71]);
    // Memory byte = 0x55
    bus.poke(0x2000, 0x55);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Result: ~0x55 = 0xAA
    let result = bus.peek(0x2000);
    assert_eq!(result, 0xAA);
    // N=1 (bit 7 set)
    assert!(cpu.regs.sr & emu_68000::N != 0);
}

#[test]
fn test_tst_memory_word() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // TST.W (A0) followed by NOP
    // Opcode: 0100 1010 01 mode reg = 0100 1010 01 010 000 = 0x4A50
    load_words(&mut bus, 0x1000, &[0x4A50, 0x4E71]);
    // Memory word = 0x0000
    bus.poke(0x2000, 0x00);
    bus.poke(0x2001, 0x00);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Memory unchanged
    let result = u16::from(bus.peek(0x2000)) << 8 | u16::from(bus.peek(0x2001));
    assert_eq!(result, 0x0000);
    // Z=1 (zero)
    assert!(cpu.regs.sr & emu_68000::Z != 0);
}

#[test]
fn test_tst_memory_negative() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // TST.W (A0) followed by NOP
    load_words(&mut bus, 0x1000, &[0x4A50, 0x4E71]);
    // Memory word = 0x8000 (negative)
    bus.poke(0x2000, 0x80);
    bus.poke(0x2001, 0x00);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.set_a(0, 0x2000);

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // Z=0, N=1
    assert!(cpu.regs.sr & emu_68000::Z == 0);
    assert!(cpu.regs.sr & emu_68000::N != 0);
}

// === Exception Tests ===

/// Helper to set up an exception vector.
/// Vector address = vector_number * 4, contains the handler address.
fn set_exception_vector(bus: &mut SimpleBus, vector: u8, handler_addr: u32) {
    let vector_addr = u32::from(vector) * 4;
    // Write handler address as big-endian long
    bus.poke(vector_addr as u16, (handler_addr >> 24) as u8);
    bus.poke((vector_addr + 1) as u16, (handler_addr >> 16) as u8);
    bus.poke((vector_addr + 2) as u16, (handler_addr >> 8) as u8);
    bus.poke((vector_addr + 3) as u16, handler_addr as u8);
}

#[test]
fn test_trap_instruction() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // Set up TRAP #0 vector (vector 32, address 0x80)
    // Handler at 0x3000
    set_exception_vector(&mut bus, 32, 0x3000);

    // TRAP #0 (opcode: 0x4E40) followed by NOP at handler
    load_words(&mut bus, 0x1000, &[0x4E40]);
    load_words(&mut bus, 0x3000, &[0x4E71]); // NOP at handler

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.sr = 0x2000; // Supervisor mode, no flags
    cpu.regs.set_a(7, 0x8000); // Set up SSP
    let old_sp = cpu.regs.a(7);

    // Run enough cycles for exception processing
    for _ in 0..50 {
        cpu.tick(&mut bus);
    }

    // PC should be at handler (or past it after NOP)
    assert!(cpu.regs.pc >= 0x3000, "PC should jump to handler, got {:#X}", cpu.regs.pc);

    // Stack should have old PC and SR pushed
    // SP decremented by 6 (4 for PC + 2 for SR)
    assert_eq!(cpu.regs.a(7), old_sp - 6);

    // Should still be in supervisor mode
    assert!(cpu.regs.sr & 0x2000 != 0, "Should be in supervisor mode");
}

#[test]
fn test_trap_15() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // Set up TRAP #15 vector (vector 47, address 0xBC)
    set_exception_vector(&mut bus, 47, 0x4000);

    // TRAP #15 (opcode: 0x4E4F) followed by NOP at handler
    load_words(&mut bus, 0x1000, &[0x4E4F]);
    load_words(&mut bus, 0x4000, &[0x4E71]); // NOP at handler

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.sr = 0x2000;
    cpu.regs.set_a(7, 0x8000); // Set up SSP

    for _ in 0..50 {
        cpu.tick(&mut bus);
    }

    // PC should be at handler
    assert!(cpu.regs.pc >= 0x4000, "PC should jump to TRAP #15 handler");
}

#[test]
fn test_division_by_zero_exception() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // Set up divide by zero vector (vector 5, address 0x14)
    set_exception_vector(&mut bus, 5, 0x5000);

    // DIVU D1,D0 (opcode: 0x80C1) - D0 / D1 where D1 = 0
    load_words(&mut bus, 0x1000, &[0x80C1]);
    load_words(&mut bus, 0x5000, &[0x4E71]); // NOP at handler

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.sr = 0x2000;
    cpu.regs.set_a(7, 0x8000); // Set up SSP
    cpu.regs.d[0] = 0x0001_0000; // Dividend
    cpu.regs.d[1] = 0x0000_0000; // Divisor = 0

    for _ in 0..50 {
        cpu.tick(&mut bus);
    }

    // PC should be at divide by zero handler
    assert!(cpu.regs.pc >= 0x5000, "PC should jump to divide by zero handler");
}

#[test]
fn test_divs_by_zero_exception() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // Set up divide by zero vector (vector 5)
    set_exception_vector(&mut bus, 5, 0x5000);

    // DIVS D1,D0 (opcode: 0x81C1) - signed D0 / D1 where D1 = 0
    load_words(&mut bus, 0x1000, &[0x81C1]);
    load_words(&mut bus, 0x5000, &[0x4E71]);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.sr = 0x2000;
    cpu.regs.set_a(7, 0x8000); // Set up SSP
    cpu.regs.d[0] = 0xFFFF_0000; // Negative dividend
    cpu.regs.d[1] = 0x0000_0000; // Divisor = 0

    for _ in 0..50 {
        cpu.tick(&mut bus);
    }

    assert!(cpu.regs.pc >= 0x5000, "PC should jump to divide by zero handler");
}

#[test]
fn test_chk_exception_negative() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // Set up CHK vector (vector 6, address 0x18)
    set_exception_vector(&mut bus, 6, 0x6000);

    // CHK D1,D0 (opcode: 0x4181) - check if D0 is in range 0..D1
    // Format: 0100 reg 110 mode reg = 0100 000 110 000 001 = 0x4181
    load_words(&mut bus, 0x1000, &[0x4181]);
    load_words(&mut bus, 0x6000, &[0x4E71]);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.sr = 0x2000;
    cpu.regs.set_a(7, 0x8000); // Set up SSP
    cpu.regs.d[0] = 0xFFFF_FFFF; // -1 (negative, out of bounds)
    cpu.regs.d[1] = 0x0000_0064; // Upper bound = 100

    for _ in 0..50 {
        cpu.tick(&mut bus);
    }

    // PC should be at CHK handler
    assert!(cpu.regs.pc >= 0x6000, "PC should jump to CHK handler for negative value");
    // N flag should be set (value was negative)
    assert!(cpu.regs.sr & emu_68000::N != 0, "N flag should be set");
}

#[test]
fn test_chk_exception_upper_bound() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // Set up CHK vector (vector 6)
    set_exception_vector(&mut bus, 6, 0x6000);

    // CHK D1,D0 - check if D0 is in range 0..D1
    load_words(&mut bus, 0x1000, &[0x4181]);
    load_words(&mut bus, 0x6000, &[0x4E71]);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.sr = 0x2000;
    cpu.regs.set_a(7, 0x8000); // Set up SSP
    cpu.regs.d[0] = 0x0000_00C8; // 200 (greater than upper bound)
    cpu.regs.d[1] = 0x0000_0064; // Upper bound = 100

    for _ in 0..50 {
        cpu.tick(&mut bus);
    }

    // PC should be at CHK handler
    assert!(cpu.regs.pc >= 0x6000, "PC should jump to CHK handler for value > upper bound");
    // N flag is undefined for upper bound violation per 68000 spec
}

#[test]
fn test_chk_no_exception() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // Set up CHK vector (vector 6)
    set_exception_vector(&mut bus, 6, 0x6000);

    // CHK D1,D0 followed by NOP
    load_words(&mut bus, 0x1000, &[0x4181, 0x4E71]);
    load_words(&mut bus, 0x6000, &[0x4E71]);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.sr = 0x2000;
    cpu.regs.d[0] = 0x0000_0032; // 50 (in bounds)
    cpu.regs.d[1] = 0x0000_0064; // Upper bound = 100

    for _ in 0..30 {
        cpu.tick(&mut bus);
    }

    // PC should NOT be at CHK handler - should continue to NOP
    assert!(cpu.regs.pc < 0x6000, "PC should not jump to CHK handler when in bounds");
}

#[test]
fn test_trapv_exception() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // Set up TRAPV vector (vector 7, address 0x1C)
    set_exception_vector(&mut bus, 7, 0x7000);

    // TRAPV (opcode: 0x4E76)
    load_words(&mut bus, 0x1000, &[0x4E76]);
    load_words(&mut bus, 0x7000, &[0x4E71]);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.sr = 0x2000 | emu_68000::V; // Set overflow flag
    cpu.regs.set_a(7, 0x8000); // Set up SSP

    for _ in 0..50 {
        cpu.tick(&mut bus);
    }

    // PC should be at TRAPV handler (V was set)
    assert!(cpu.regs.pc >= 0x7000, "PC should jump to TRAPV handler when V is set");
}

#[test]
fn test_trapv_no_exception() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // Set up TRAPV vector (vector 7)
    set_exception_vector(&mut bus, 7, 0x7000);

    // TRAPV followed by NOP
    load_words(&mut bus, 0x1000, &[0x4E76, 0x4E71]);
    load_words(&mut bus, 0x7000, &[0x4E71]);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.sr = 0x2000; // V flag clear

    for _ in 0..30 {
        cpu.tick(&mut bus);
    }

    // PC should NOT jump to handler (V was clear)
    assert!(cpu.regs.pc < 0x7000, "PC should not jump to TRAPV handler when V is clear");
}

#[test]
fn test_privilege_violation_move_to_sr() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // Set up privilege violation vector (vector 8, address 0x20)
    set_exception_vector(&mut bus, 8, 0x8000);

    // MOVE D0,SR (opcode: 0x46C0) - privileged instruction
    load_words(&mut bus, 0x1000, &[0x46C0]);
    load_words(&mut bus, 0x8000, &[0x4E71]);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.sr = 0x0000; // User mode (S bit clear)
    // Note: In user mode, A7 is USP. But exception switches to supervisor and uses SSP.
    // SSP starts at 0 after reset, so we need to set it up via register direct access.
    cpu.regs.set_a(7, 0x8000); // This sets USP since we're in user mode
    // We also need to set SSP for when the exception occurs
    // The 68000 has separate USP and SSP, and exception uses SSP
    cpu.regs.d[0] = 0x0000_2700;

    for _ in 0..50 {
        cpu.tick(&mut bus);
    }

    // PC should be at privilege violation handler
    assert!(cpu.regs.pc >= 0x8000, "PC should jump to privilege violation handler");
    // Should now be in supervisor mode
    assert!(cpu.regs.sr & 0x2000 != 0, "Should be in supervisor mode after exception");
}

#[test]
fn test_illegal_instruction_exception() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // Set up illegal instruction vector (vector 4, address 0x10)
    set_exception_vector(&mut bus, 4, 0x4000);

    // Illegal instruction: 0x4AFC is the official ILLEGAL opcode
    load_words(&mut bus, 0x1000, &[0x4AFC]);
    load_words(&mut bus, 0x4000, &[0x4E71]);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.sr = 0x2000;
    cpu.regs.set_a(7, 0x8000); // Set up SSP

    for _ in 0..50 {
        cpu.tick(&mut bus);
    }

    // PC should be at illegal instruction handler
    assert!(cpu.regs.pc >= 0x4000, "PC should jump to illegal instruction handler");
}

#[test]
fn test_line_a_exception() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // Set up Line A vector (vector 10, address 0x28)
    set_exception_vector(&mut bus, 10, 0xA000);

    // Line A instruction: 0xAxxx (any instruction starting with 0xA)
    load_words(&mut bus, 0x1000, &[0xA123]);
    load_words(&mut bus, 0xA000, &[0x4E71]);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.sr = 0x2000;
    cpu.regs.set_a(7, 0x8000); // Set up SSP

    for _ in 0..50 {
        cpu.tick(&mut bus);
    }

    // PC should be at Line A handler
    assert!(cpu.regs.pc >= 0xA000, "PC should jump to Line A handler");
}

#[test]
fn test_line_f_exception() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // Set up Line F vector (vector 11, address 0x2C)
    set_exception_vector(&mut bus, 11, 0xF000);

    // Line F instruction: 0xFxxx (any instruction starting with 0xF)
    load_words(&mut bus, 0x1000, &[0xF123]);
    load_words(&mut bus, 0xF000, &[0x4E71]);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.sr = 0x2000;
    cpu.regs.set_a(7, 0x8000); // Set up SSP

    for _ in 0..50 {
        cpu.tick(&mut bus);
    }

    // PC should be at Line F handler
    assert!(cpu.regs.pc >= 0xF000, "PC should jump to Line F handler");
}

#[test]
fn test_exception_saves_state() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // Set up TRAP #0 vector
    set_exception_vector(&mut bus, 32, 0x3000);

    // TRAP #0
    load_words(&mut bus, 0x1000, &[0x4E40]);
    load_words(&mut bus, 0x3000, &[0x4E71]);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    // Start in supervisor mode for simpler testing
    cpu.regs.sr = 0x201F; // Supervisor mode with all CCR flags set
    cpu.regs.set_a(7, 0x8000); // SSP

    let expected_return_pc = 0x1002u32;
    let expected_old_sr = 0x201Fu16;
    let initial_sp = cpu.regs.a(7);

    for _ in 0..50 {
        cpu.tick(&mut bus);
    }

    // SP should be decremented by 6 (PC=4, SR=2)
    let final_sp = cpu.regs.a(7);
    assert_eq!(final_sp, initial_sp - 6, "Stack should be decremented by 6");

    // Read pushed values from stack
    // 68000 exception frame: SR (2 bytes) at SP, then PC (4 bytes) at SP+2
    // Stack grows down, SP points to top of stack (lowest used address)
    // Push order: PC first (SP-=4), then SR (SP-=2)
    // Final layout: SP+0=SR, SP+2=PC

    // Read pushed SR (at SP)
    let pushed_sr = u16::from(bus.peek(final_sp as u16)) << 8
        | u16::from(bus.peek((final_sp + 1) as u16));

    // Read pushed PC (at SP + 2)
    let pushed_pc = u32::from(bus.peek((final_sp + 2) as u16)) << 24
        | u32::from(bus.peek((final_sp + 3) as u16)) << 16
        | u32::from(bus.peek((final_sp + 4) as u16)) << 8
        | u32::from(bus.peek((final_sp + 5) as u16));

    assert_eq!(pushed_sr, expected_old_sr, "Pushed SR should be original SR");
    assert_eq!(pushed_pc, expected_return_pc, "Pushed PC should be return address");
}

// =============================================================================
// ROXL/ROXR - Rotate through Extend
// =============================================================================

#[test]
fn test_roxl_byte_with_x_clear() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ROXL.B #1,D0 (opcode: 0xE310)
    // Rotate left through X by 1 bit
    load_words(&mut bus, 0x1000, &[0xE310, 0x4E71]);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x80; // 1000_0000 - MSB set
    cpu.regs.sr &= !emu_68000::X; // X clear

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // After ROXL.B #1 with X=0:
    // 0x80 = 1000_0000, X=0
    // Rotate left: MSB (1) goes to X and C, old X (0) goes to LSB
    // Result: 0000_0000 = 0x00, X=1, C=1
    assert_eq!(cpu.regs.d[0] & 0xFF, 0x00);
    assert!(cpu.regs.sr & emu_68000::X != 0, "X should be set");
    assert!(cpu.regs.sr & emu_68000::C != 0, "C should be set");
}

#[test]
fn test_roxl_byte_with_x_set() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ROXL.B #1,D0 (opcode: 0xE310)
    load_words(&mut bus, 0x1000, &[0xE310, 0x4E71]);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x40; // 0100_0000
    cpu.regs.sr |= emu_68000::X; // X set

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // After ROXL.B #1 with X=1:
    // 0x40 = 0100_0000, X=1
    // Rotate left: MSB (0) goes to X and C, old X (1) goes to LSB
    // Result: 1000_0001 = 0x81, X=0, C=0
    assert_eq!(cpu.regs.d[0] & 0xFF, 0x81);
    assert!(cpu.regs.sr & emu_68000::X == 0, "X should be clear");
    assert!(cpu.regs.sr & emu_68000::C == 0, "C should be clear");
}

#[test]
fn test_roxr_byte_with_x_clear() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ROXR.B #1,D0 (opcode: 0xE210)
    load_words(&mut bus, 0x1000, &[0xE210, 0x4E71]);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x01; // 0000_0001 - LSB set
    cpu.regs.sr &= !emu_68000::X; // X clear

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // After ROXR.B #1 with X=0:
    // 0x01 = 0000_0001, X=0
    // Rotate right: LSB (1) goes to X and C, old X (0) goes to MSB
    // Result: 0000_0000 = 0x00, X=1, C=1
    assert_eq!(cpu.regs.d[0] & 0xFF, 0x00);
    assert!(cpu.regs.sr & emu_68000::X != 0, "X should be set");
    assert!(cpu.regs.sr & emu_68000::C != 0, "C should be set");
}

#[test]
fn test_roxr_byte_with_x_set() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ROXR.B #1,D0 (opcode: 0xE210)
    load_words(&mut bus, 0x1000, &[0xE210, 0x4E71]);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x02; // 0000_0010
    cpu.regs.sr |= emu_68000::X; // X set

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // After ROXR.B #1 with X=1:
    // 0x02 = 0000_0010, X=1
    // Rotate right: LSB (0) goes to X and C, old X (1) goes to MSB
    // Result: 1000_0001 = 0x81, X=0, C=0
    assert_eq!(cpu.regs.d[0] & 0xFF, 0x81);
    assert!(cpu.regs.sr & emu_68000::X == 0, "X should be clear");
    assert!(cpu.regs.sr & emu_68000::C == 0, "C should be clear");
}

#[test]
fn test_roxl_word_count_2() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // ROXL.W #2,D0 (opcode: 0xE550)
    load_words(&mut bus, 0x1000, &[0xE550, 0x4E71]);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0xC000; // 1100_0000_0000_0000
    cpu.regs.sr |= emu_68000::X; // X=1

    for _ in 0..20 {
        cpu.tick(&mut bus);
    }

    // After ROXL.W #2 with X=1:
    // Initial: X=1, value=1100_0000_0000_0000
    // Step 1: MSB(1)->X, old_X(1)->LSB: X=1, value=1000_0000_0000_0001
    // Step 2: MSB(1)->X, old_X(1)->LSB: X=1, value=0000_0000_0000_0011
    assert_eq!(cpu.regs.d[0] & 0xFFFF, 0x0003);
    assert!(cpu.regs.sr & emu_68000::X != 0, "X should be set");
}

// =============================================================================
// MOVEP - Move Peripheral Data
// =============================================================================

#[test]
fn test_movep_word_to_memory() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MOVEP.W D0,2(A0) (opcode: 0x0188 + displacement 0x0002)
    // Encoding: 0000_rrr1_1000_1aaa = 0x0188 for D0,d(A0)
    load_words(&mut bus, 0x1000, &[0x0188, 0x0002, 0x4E71]);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0x1234; // Value to write (low word)
    cpu.regs.set_a(0, 0x2000); // Base address

    for _ in 0..40 {
        cpu.tick(&mut bus);
    }

    // MOVEP.W writes high byte to d(An), low byte to d(An)+2
    // D0.W = 0x1234, so high byte 0x12 to 0x2002, low byte 0x34 to 0x2004
    assert_eq!(bus.peek(0x2002), 0x12, "High byte should be at base+disp");
    assert_eq!(bus.peek(0x2004), 0x34, "Low byte should be at base+disp+2");
}

#[test]
fn test_movep_long_to_memory() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MOVEP.L D1,4(A1) (opcode: 0x03C9 + displacement 0x0004)
    // Encoding: 0000_xxx1_mm00_1yyy where xxx=001(D1), mm=11(long,reg->mem), yyy=001(A1)
    // = 0000_0011_1100_1001 = 0x03C9
    load_words(&mut bus, 0x1000, &[0x03C9, 0x0004, 0x4E71]);

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[1] = 0xDEADBEEF; // Value to write
    cpu.regs.set_a(1, 0x2000); // Base address

    for _ in 0..50 {
        cpu.tick(&mut bus);
    }

    // MOVEP.L writes bytes to d(An), d(An)+2, d(An)+4, d(An)+6
    // D1 = 0xDEADBEEF
    assert_eq!(bus.peek(0x2004), 0xDE, "Byte 3 (MSB)");
    assert_eq!(bus.peek(0x2006), 0xAD, "Byte 2");
    assert_eq!(bus.peek(0x2008), 0xBE, "Byte 1");
    assert_eq!(bus.peek(0x200A), 0xEF, "Byte 0 (LSB)");
}

#[test]
fn test_movep_word_from_memory() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MOVEP.W 2(A0),D0 (opcode: 0x0108 + displacement 0x0002)
    // Encoding: 0000_rrr1_0000_1aaa = 0x0108 for d(A0),D0
    load_words(&mut bus, 0x1000, &[0x0108, 0x0002, 0x4E71]);

    // Set up memory bytes at alternate addresses
    bus.poke(0x2002, 0xAB); // High byte
    bus.poke(0x2004, 0xCD); // Low byte

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[0] = 0xFFFF_0000; // Pre-fill upper word
    cpu.regs.set_a(0, 0x2000); // Base address

    for _ in 0..40 {
        cpu.tick(&mut bus);
    }

    // MOVEP.W reads from d(An) and d(An)+2 into low word of Dn
    // Result: 0xABCD in low word, upper word unchanged
    assert_eq!(cpu.regs.d[0], 0xFFFF_ABCD);
}

#[test]
fn test_movep_long_from_memory() {
    let mut cpu = M68000::new();
    let mut bus = SimpleBus::new();

    // MOVEP.L 0(A2),D2 (opcode: 0x054A + displacement 0x0000)
    // Encoding: 0000_xxx1_mm00_1yyy where xxx=010(D2), mm=01(long,mem->reg), yyy=010(A2)
    // = 0000_0101_0100_1010 = 0x054A
    load_words(&mut bus, 0x1000, &[0x054A, 0x0000, 0x4E71]);

    // Set up memory bytes at alternate addresses
    bus.poke(0x3000, 0x12); // Byte 3 (MSB)
    bus.poke(0x3002, 0x34); // Byte 2
    bus.poke(0x3004, 0x56); // Byte 1
    bus.poke(0x3006, 0x78); // Byte 0 (LSB)

    cpu.reset();
    cpu.regs.pc = 0x1000;
    cpu.regs.d[2] = 0; // Clear
    cpu.regs.set_a(2, 0x3000); // Base address

    for _ in 0..50 {
        cpu.tick(&mut bus);
    }

    // MOVEP.L reads 4 bytes into Dn
    assert_eq!(cpu.regs.d[2], 0x12345678);
}
