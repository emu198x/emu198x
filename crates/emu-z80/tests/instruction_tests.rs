//! Unit tests for individual Z80 instructions.
//!
//! These tests verify each instruction works correctly in isolation
//! before running comprehensive tests like ZEXDOC.

use emu_core::{Cpu, SimpleBus};
use emu_z80::Z80;

/// Run CPU until it HALTs, return instruction count.
fn run_until_halt(cpu: &mut Z80, bus: &mut SimpleBus) -> u64 {
    let mut count = 0;
    while !cpu.is_halted() && count < 10000 {
        cpu.tick(bus);
        count += 1;
    }
    count
}

/// Run a specific number of ticks.
fn run_ticks(cpu: &mut Z80, bus: &mut SimpleBus, ticks: u64) {
    for _ in 0..ticks {
        cpu.tick(bus);
    }
}

#[test]
fn test_nop() {
    let mut bus = SimpleBus::new();
    bus.load(0x0000, &[0x00, 0x76]); // NOP, HALT

    let mut cpu = Z80::new();
    cpu.set_pc(0x0000);

    run_until_halt(&mut cpu, &mut bus);

    assert_eq!(cpu.pc(), 0x0002); // After HALT
}

#[test]
fn test_ld_a_n() {
    let mut bus = SimpleBus::new();
    bus.load(0x0000, &[0x3E, 0x42, 0x76]); // LD A, 0x42; HALT

    let mut cpu = Z80::new();
    cpu.set_pc(0x0000);

    run_until_halt(&mut cpu, &mut bus);

    assert_eq!(cpu.a(), 0x42);
}

#[test]
fn test_ld_bc_nn() {
    let mut bus = SimpleBus::new();
    bus.load(0x0000, &[0x01, 0x34, 0x12, 0x76]); // LD BC, 0x1234; HALT

    let mut cpu = Z80::new();
    cpu.set_pc(0x0000);

    run_until_halt(&mut cpu, &mut bus);

    assert_eq!(cpu.bc(), 0x1234);
}

#[test]
fn test_push_pop_bc() {
    let mut bus = SimpleBus::new();
    // LD BC, 0x1234; LD SP, 0x8000; PUSH BC; LD BC, 0; POP BC; HALT
    bus.load(0x0000, &[
        0x01, 0x34, 0x12,       // LD BC, 0x1234
        0x31, 0x00, 0x80,       // LD SP, 0x8000
        0xC5,                   // PUSH BC
        0x01, 0x00, 0x00,       // LD BC, 0x0000
        0xC1,                   // POP BC
        0x76,                   // HALT
    ]);

    let mut cpu = Z80::new();
    cpu.set_pc(0x0000);

    run_until_halt(&mut cpu, &mut bus);

    assert_eq!(cpu.bc(), 0x1234, "BC should be restored after PUSH/POP");
    assert_eq!(cpu.sp(), 0x8000, "SP should be back to original");
}

#[test]
fn test_call_ret() {
    let mut bus = SimpleBus::new();
    // Main: LD SP, 0x8000; CALL 0x0010; LD A, 0x99; HALT
    // Subroutine at 0x0010: LD A, 0x42; RET
    bus.load(0x0000, &[
        0x31, 0x00, 0x80,       // LD SP, 0x8000
        0xCD, 0x10, 0x00,       // CALL 0x0010
        0x3E, 0x99,             // LD A, 0x99 (after return)
        0x76,                   // HALT
    ]);
    bus.load(0x0010, &[
        0x3E, 0x42,             // LD A, 0x42
        0xC9,                   // RET
    ]);

    let mut cpu = Z80::new();
    cpu.set_pc(0x0000);

    run_until_halt(&mut cpu, &mut bus);

    assert_eq!(cpu.a(), 0x99, "A should be 0x99 (set after RET)");
    assert_eq!(cpu.sp(), 0x8000, "SP should be restored after CALL/RET");
}

#[test]
fn test_nested_call_ret() {
    let mut bus = SimpleBus::new();
    // Main: LD SP, 0x8000; CALL 0x0020; HALT
    // Sub1 at 0x0020: LD A, 1; CALL 0x0030; ADD A, 10; RET
    // Sub2 at 0x0030: ADD A, 100; RET
    bus.load(0x0000, &[
        0x31, 0x00, 0x80,       // LD SP, 0x8000
        0xCD, 0x20, 0x00,       // CALL 0x0020
        0x76,                   // HALT
    ]);
    bus.load(0x0020, &[
        0x3E, 0x01,             // LD A, 1
        0xCD, 0x30, 0x00,       // CALL 0x0030
        0xC6, 0x0A,             // ADD A, 10
        0xC9,                   // RET
    ]);
    bus.load(0x0030, &[
        0xC6, 0x64,             // ADD A, 100
        0xC9,                   // RET
    ]);

    let mut cpu = Z80::new();
    cpu.set_pc(0x0000);

    run_until_halt(&mut cpu, &mut bus);

    // A = 1, then +100 in sub2, then +10 in sub1 = 111
    assert_eq!(cpu.a(), 111, "A should be 111 (1 + 100 + 10)");
    assert_eq!(cpu.sp(), 0x8000, "SP should be restored after nested calls");
}

#[test]
fn test_jr_unconditional() {
    let mut bus = SimpleBus::new();
    // JR +2 (skip next 2 bytes), then unreachable, then LD A, 0x42; HALT
    bus.load(0x0000, &[
        0x18, 0x02,             // JR +2
        0x3E, 0xFF,             // LD A, 0xFF (should be skipped)
        0x3E, 0x42,             // LD A, 0x42
        0x76,                   // HALT
    ]);

    let mut cpu = Z80::new();
    cpu.set_pc(0x0000);

    run_until_halt(&mut cpu, &mut bus);

    assert_eq!(cpu.a(), 0x42, "A should be 0x42 (skipped 0xFF)");
}

#[test]
fn test_djnz_loop() {
    let mut bus = SimpleBus::new();
    // LD B, 5; LD A, 0; loop: INC A; DJNZ loop; HALT
    bus.load(0x0000, &[
        0x06, 0x05,             // LD B, 5
        0x3E, 0x00,             // LD A, 0
        // loop at 0x0004:
        0x3C,                   // INC A
        0x10, 0xFD,             // DJNZ -3 (back to INC A)
        0x76,                   // HALT
    ]);

    let mut cpu = Z80::new();
    cpu.set_pc(0x0000);

    run_until_halt(&mut cpu, &mut bus);

    assert_eq!(cpu.a(), 5, "A should be 5 after loop");
    assert_eq!(cpu.bc() >> 8, 0, "B should be 0 after loop");
}

#[test]
fn test_ld_hl_from_memory() {
    let mut bus = SimpleBus::new();
    // Store 0x1234 at 0x0050, then LD HL, (0x0050); HALT
    bus.load(0x0050, &[0x34, 0x12]); // Little-endian: 0x1234
    bus.load(0x0000, &[
        0x2A, 0x50, 0x00,       // LD HL, (0x0050)
        0x76,                   // HALT
    ]);

    let mut cpu = Z80::new();
    cpu.set_pc(0x0000);

    run_until_halt(&mut cpu, &mut bus);

    assert_eq!(cpu.hl(), 0x1234, "HL should be loaded from memory");
}

#[test]
fn test_ld_sp_nn() {
    let mut bus = SimpleBus::new();
    bus.load(0x0000, &[
        0x31, 0x34, 0x12,       // LD SP, 0x1234
        0x76,                   // HALT
    ]);

    let mut cpu = Z80::new();
    cpu.set_pc(0x0000);

    run_until_halt(&mut cpu, &mut bus);

    assert_eq!(cpu.sp(), 0x1234, "SP should be 0x1234");
}

#[test]
fn test_ld_nn_sp() {
    // ED 73 nn nn - LD (nn), SP
    let mut bus = SimpleBus::new();
    bus.load(0x0000, &[
        0x31, 0x34, 0x12,       // LD SP, 0x1234
        0xED, 0x73, 0x50, 0x00, // LD (0x0050), SP
        0x76,                   // HALT
    ]);

    let mut cpu = Z80::new();
    cpu.set_pc(0x0000);

    run_until_halt(&mut cpu, &mut bus);

    // Check memory at 0x0050 contains 0x1234 (little-endian)
    assert_eq!(bus.peek(0x0050), 0x34, "Low byte of SP");
    assert_eq!(bus.peek(0x0051), 0x12, "High byte of SP");
}

#[test]
fn test_ld_sp_from_memory() {
    // ED 7B nn nn - LD SP, (nn)
    let mut bus = SimpleBus::new();
    bus.load(0x0050, &[0x34, 0x12]); // 0x1234 at 0x0050
    bus.load(0x0000, &[
        0xED, 0x7B, 0x50, 0x00, // LD SP, (0x0050)
        0x76,                   // HALT
    ]);

    let mut cpu = Z80::new();
    cpu.set_pc(0x0000);

    run_until_halt(&mut cpu, &mut bus);

    assert_eq!(cpu.sp(), 0x1234, "SP should be loaded from memory");
}

#[test]
fn test_save_restore_sp() {
    // Simulate what ZEXDOC does: save SP, set new SP, work, restore SP
    let mut bus = SimpleBus::new();
    bus.load(0x0000, &[
        0x31, 0x00, 0x80,       // LD SP, 0x8000 (initial SP)
        0xED, 0x73, 0x50, 0x00, // LD (0x0050), SP - save original SP
        0x31, 0x00, 0x70,       // LD SP, 0x7000 (new working SP)
        // ... do some work with new SP ...
        0xC5,                   // PUSH BC (uses SP=0x7000)
        0xC1,                   // POP BC
        // ... end work ...
        0xED, 0x7B, 0x50, 0x00, // LD SP, (0x0050) - restore original SP
        0x76,                   // HALT
    ]);

    let mut cpu = Z80::new();
    cpu.set_pc(0x0000);

    run_until_halt(&mut cpu, &mut bus);

    assert_eq!(cpu.sp(), 0x8000, "SP should be restored to original");
}

#[test]
fn test_ex_de_hl() {
    let mut bus = SimpleBus::new();
    bus.load(0x0000, &[
        0x21, 0x34, 0x12,       // LD HL, 0x1234
        0x11, 0x78, 0x56,       // LD DE, 0x5678
        0xEB,                   // EX DE, HL
        0x76,                   // HALT
    ]);

    let mut cpu = Z80::new();
    cpu.set_pc(0x0000);

    run_until_halt(&mut cpu, &mut bus);

    assert_eq!(cpu.hl(), 0x5678, "HL should have DE's value");
    assert_eq!(cpu.de(), 0x1234, "DE should have HL's value");
}

#[test]
fn test_add_hl_de() {
    let mut bus = SimpleBus::new();
    bus.load(0x0000, &[
        0x21, 0x00, 0x10,       // LD HL, 0x1000
        0x11, 0x34, 0x12,       // LD DE, 0x1234
        0x19,                   // ADD HL, DE
        0x76,                   // HALT
    ]);

    let mut cpu = Z80::new();
    cpu.set_pc(0x0000);

    run_until_halt(&mut cpu, &mut bus);

    assert_eq!(cpu.hl(), 0x2234, "HL should be 0x1000 + 0x1234 = 0x2234");
}
