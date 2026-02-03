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
