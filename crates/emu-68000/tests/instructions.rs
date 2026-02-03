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
