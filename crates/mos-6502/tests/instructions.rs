//! Unit tests for 6502 instruction behavior.

use emu_core::{Bus, Cpu, SimpleBus};
use mos_6502::{Mos6502, flags};

/// Run one complete instruction (fetch + execute cycles).
fn run_instruction(cpu: &mut Mos6502, bus: &mut SimpleBus) {
    // First tick does opcode fetch, transitioning out of FetchOpcode
    cpu.tick(bus);

    // Now run until we return to FetchOpcode (instruction complete)
    for _ in 0..20 {
        if cpu.is_instruction_complete() {
            return;
        }
        cpu.tick(bus);
    }
    panic!("Instruction did not complete within 20 cycles");
}

/// Load a program at $0200 and set PC there.
fn setup_program(bus: &mut SimpleBus, cpu: &mut Mos6502, program: &[u8]) {
    bus.load(0x0200, program);
    cpu.regs.pc = 0x0200;
}

#[test]
fn test_stack_pha_pla() {
    let mut bus = SimpleBus::new();
    let mut cpu = Mos6502::new();

    // LDA #$42; LDX #$FF; TXS; PHA; LDA #$00; PLA
    let program = [
        0xA9, 0x42, // LDA #$42
        0xA2, 0xFF, // LDX #$FF
        0x9A, // TXS
        0x48, // PHA
        0xA9, 0x00, // LDA #$00
        0x68, // PLA
    ];
    setup_program(&mut bus, &mut cpu, &program);

    for _ in 0..6 {
        run_instruction(&mut cpu, &mut bus);
    }

    assert_eq!(cpu.regs.a, 0x42, "PLA should restore A");
    assert_eq!(cpu.regs.s, 0xFF, "SP should be back to $FF after PLA");
}

#[test]
fn test_stack_php_plp() {
    let mut bus = SimpleBus::new();
    let mut cpu = Mos6502::new();

    // Set up: LDX #$FF; TXS; SEC; PHP; CLC; PLP
    let program = [
        0xA2, 0xFF, // LDX #$FF
        0x9A, // TXS
        0x38, // SEC (set carry)
        0x08, // PHP
        0x18, // CLC (clear carry)
        0x28, // PLP
    ];
    setup_program(&mut bus, &mut cpu, &program);

    for _ in 0..6 {
        run_instruction(&mut cpu, &mut bus);
    }

    assert!(cpu.regs.p.is_set(flags::C), "PLP should restore carry flag");
    assert_eq!(cpu.regs.s, 0xFF, "SP should be back to $FF");
}

#[test]
fn test_brk_stack_layout() {
    let mut bus = SimpleBus::new();
    let mut cpu = Mos6502::new();

    // Set up BRK vector to point to $0300
    bus.write(0xFFFE, 0x00);
    bus.write(0xFFFF, 0x03);

    // Program: LDX #$FF; TXS; CLI; BRK; padding
    // Start at $0200
    let program = [
        0xA2, 0xFF, // LDX #$FF    @ $0200
        0x9A, // TXS         @ $0202
        0x58, // CLI         @ $0203 (clear I flag)
        0x00, // BRK         @ $0204
        0xEA, // NOP padding @ $0205 (this byte is skipped)
    ];
    setup_program(&mut bus, &mut cpu, &program);

    // Run LDX, TXS, CLI, BRK
    for _ in 0..4 {
        run_instruction(&mut cpu, &mut bus);
    }

    // After BRK:
    // - PC should be at $0300 (from vector)
    // - Stack should have: PCH=$02, PCL=$06 (PC after padding), P=$30 (U+B, no I)
    // - SP should be $FC

    assert_eq!(cpu.pc(), 0x0300, "PC should be at BRK vector target");
    assert_eq!(
        cpu.regs.s, 0xFC,
        "SP should be $FC after BRK (3 pushes from $FF)"
    );
    assert!(
        cpu.regs.p.is_set(flags::I),
        "I flag should be set after BRK"
    );

    // Check stack contents
    let pushed_pch = bus.peek(0x01FF);
    let pushed_pcl = bus.peek(0x01FE);
    let pushed_p = bus.peek(0x01FD);

    // Return address is PC after BRK's operand byte
    // BRK at $0204, padding byte at $0205, return address = $0206
    assert_eq!(pushed_pch, 0x02, "Pushed PCH should be $02");
    assert_eq!(pushed_pcl, 0x06, "Pushed PCL should be $06");

    // Pushed P should have B and U set, but NOT I (since we did CLI before BRK)
    // B=0x10, U=0x20, so expect $30
    assert_eq!(pushed_p & 0x30, 0x30, "Pushed P should have B and U set");
    assert_eq!(
        pushed_p & 0x04,
        0x00,
        "Pushed P should NOT have I set (we did CLI)"
    );

    eprintln!("BRK test: PC=${:04X}, SP=${:02X}", cpu.pc(), cpu.regs.s);
    eprintln!(
        "Stack: PCH=${:02X}, PCL=${:02X}, P=${:02X}",
        pushed_pch, pushed_pcl, pushed_p
    );
}

#[test]
fn test_brk_dormann_sequence() {
    // Mimics the exact sequence from Dormann test at $37C9 failure
    let mut bus = SimpleBus::new();
    let mut cpu = Mos6502::new();

    // BRK vector points to handler at $0300
    bus.write(0xFFFE, 0x00);
    bus.write(0xFFFF, 0x03);

    // Handler at $0300: PHP; (more code)
    bus.write(0x0300, 0x08); // PHP
    bus.write(0x0301, 0x60); // RTS (just to end)

    // Main program that mimics Dormann's setup
    let program = [
        0xA2, 0xFF, // LDX #$FF    @ $0200 - set SP to $FF
        0x9A, // TXS         @ $0202
        0xA9, 0x00, // LDA #$00    @ $0203 - load 0 for PHA
        0x48, // PHA         @ $0205 - push 0 to $01FF, SP=$FE
        0xA9, 0x42, // LDA #$42    @ $0206 - destroy A
        0x28, // PLP         @ $0208 - pull status (clears flags), SP=$FF
        0x00, // BRK         @ $0209
        0xEA, // NOP padding @ $020A
    ];
    setup_program(&mut bus, &mut cpu, &program);

    // Run up through BRK
    eprintln!("\n=== Running Dormann-like BRK sequence ===");
    let names = [
        "LDX #$FF", "TXS", "LDA #$00", "PHA", "LDA #$42", "PLP", "BRK",
    ];
    for name in &names {
        let pc_before = cpu.pc();
        run_instruction(&mut cpu, &mut bus);
        eprintln!(
            "{}: PC ${:04X}->${:04X}, SP=${:02X}, P=${:02X}",
            name,
            pc_before,
            cpu.pc(),
            cpu.regs.s,
            cpu.regs.p.0
        );
    }

    // Now at BRK handler ($0300), run PHP
    let pc_before = cpu.pc();
    run_instruction(&mut cpu, &mut bus);
    eprintln!(
        "PHP: PC ${:04X}->${:04X}, SP=${:02X}",
        pc_before,
        cpu.pc(),
        cpu.regs.s
    );

    // After sequence:
    // LDX #$FF; TXS -> SP=$FF
    // PHA -> push to $01FF, SP=$FE
    // PLP -> pull from $01FF, SP=$FF
    // BRK -> push PCH to $01FF, PCL to $01FE, P to $01FD, SP=$FC
    // PHP -> push P to $01FC, SP=$FB

    assert_eq!(cpu.regs.s, 0xFB, "SP should be $FB after BRK+PHP");

    // The pushed status from BRK should be at $01FD
    let brk_pushed_p = bus.peek(0x01FD);
    eprintln!("BRK pushed P (at $01FD): ${:02X}", brk_pushed_p);

    // Since PLP restored status from the 0x00 we pushed, the status at BRK
    // should have been $00 | U = $20 (PLP ignores B, sets U)
    // BRK adds B, so pushed should be $30
    assert_eq!(brk_pushed_p, 0x30, "BRK should push P with U+B ($30)");

    // Dump stack for debugging
    eprintln!("\nStack contents:");
    for i in 0..8 {
        let addr = 0x01F8_u16 + i;
        eprintln!("  ${:04X}: ${:02X}", addr, bus.peek(addr));
    }
}

// ============================================================================
// Stale State Prevention Tests
//
// These tests verify that BRK uses the correct vector ($FFFE) regardless of
// what addressing modes were used in preceding instructions. This catches
// bugs where temporary state (like `addr`) leaks between instructions.
// ============================================================================

/// Helper to set up BRK vector and stack for stale state tests.
fn setup_brk_test(bus: &mut SimpleBus, cpu: &mut Mos6502) {
    // BRK vector points to $0300
    bus.write(0xFFFE, 0x00);
    bus.write(0xFFFF, 0x03);

    // Set up stack
    cpu.regs.s = 0xFF;
}

#[test]
fn test_brk_after_absolute_addressing() {
    // Regression test: LDA $1234 sets addr = $1234
    // BRK must NOT use that stale value as its vector
    let mut bus = SimpleBus::new();
    let mut cpu = Mos6502::new();
    setup_brk_test(&mut bus, &mut cpu);

    // Put some data at $1234 so LDA has something to read
    bus.write(0x1234, 0x42);

    let program = [
        0xAD, 0x34, 0x12, // LDA $1234  (sets internal addr to $1234)
        0x00, // BRK        (must use $FFFE, not $1234)
        0xEA, // NOP padding
    ];
    setup_program(&mut bus, &mut cpu, &program);

    // Run LDA, BRK
    run_instruction(&mut cpu, &mut bus);
    run_instruction(&mut cpu, &mut bus);

    assert_eq!(
        cpu.pc(),
        0x0300,
        "BRK must jump to vector at $FFFE ($0300), not use stale addr from LDA $1234"
    );
}

#[test]
fn test_brk_after_absolute_x_addressing() {
    // LDA $1000,X with X=$34 sets addr during calculation
    let mut bus = SimpleBus::new();
    let mut cpu = Mos6502::new();
    setup_brk_test(&mut bus, &mut cpu);

    bus.write(0x1034, 0x42); // $1000 + $34

    let program = [
        0xA2, 0x34, // LDX #$34
        0xBD, 0x00, 0x10, // LDA $1000,X (effective = $1034)
        0x00, // BRK
        0xEA, // NOP padding
    ];
    setup_program(&mut bus, &mut cpu, &program);

    for _ in 0..3 {
        run_instruction(&mut cpu, &mut bus);
    }

    assert_eq!(
        cpu.pc(),
        0x0300,
        "BRK must use correct vector after LDA abs,X"
    );
}

#[test]
fn test_brk_after_absolute_y_addressing() {
    // LDA $1000,Y with Y=$56 sets addr during calculation
    let mut bus = SimpleBus::new();
    let mut cpu = Mos6502::new();
    setup_brk_test(&mut bus, &mut cpu);

    bus.write(0x1056, 0x42);

    let program = [
        0xA0, 0x56, // LDY #$56
        0xB9, 0x00, 0x10, // LDA $1000,Y (effective = $1056)
        0x00, // BRK
        0xEA, // NOP padding
    ];
    setup_program(&mut bus, &mut cpu, &program);

    for _ in 0..3 {
        run_instruction(&mut cpu, &mut bus);
    }

    assert_eq!(
        cpu.pc(),
        0x0300,
        "BRK must use correct vector after LDA abs,Y"
    );
}

#[test]
fn test_brk_after_indirect_indexed() {
    // LDA ($80),Y uses addr for the indirect pointer calculation
    let mut bus = SimpleBus::new();
    let mut cpu = Mos6502::new();
    setup_brk_test(&mut bus, &mut cpu);

    // Set up pointer at $80/$81 pointing to $2000
    bus.write(0x0080, 0x00);
    bus.write(0x0081, 0x20);
    // Data at $2000 + Y
    bus.write(0x2010, 0x42);

    let program = [
        0xA0, 0x10, // LDY #$10
        0xB1, 0x80, // LDA ($80),Y (effective = $2010)
        0x00, // BRK
        0xEA, // NOP padding
    ];
    setup_program(&mut bus, &mut cpu, &program);

    for _ in 0..3 {
        run_instruction(&mut cpu, &mut bus);
    }

    assert_eq!(
        cpu.pc(),
        0x0300,
        "BRK must use correct vector after LDA (zp),Y"
    );
}

#[test]
fn test_brk_after_indexed_indirect() {
    // LDA ($80,X) uses pointer and addr
    let mut bus = SimpleBus::new();
    let mut cpu = Mos6502::new();
    setup_brk_test(&mut bus, &mut cpu);

    // Set up pointer at $90/$91 (after $80 + X=$10)
    bus.write(0x0090, 0x00);
    bus.write(0x0091, 0x30);
    // Data at $3000
    bus.write(0x3000, 0x42);

    let program = [
        0xA2, 0x10, // LDX #$10
        0xA1, 0x80, // LDA ($80,X) (pointer at $90, effective = $3000)
        0x00, // BRK
        0xEA, // NOP padding
    ];
    setup_program(&mut bus, &mut cpu, &program);

    for _ in 0..3 {
        run_instruction(&mut cpu, &mut bus);
    }

    assert_eq!(
        cpu.pc(),
        0x0300,
        "BRK must use correct vector after LDA (zp,X)"
    );
}

#[test]
fn test_brk_after_rmw_absolute() {
    // INC $1234 uses addr for the read-modify-write
    let mut bus = SimpleBus::new();
    let mut cpu = Mos6502::new();
    setup_brk_test(&mut bus, &mut cpu);

    bus.write(0x1234, 0x41);

    let program = [
        0xEE, 0x34, 0x12, // INC $1234
        0x00, // BRK
        0xEA, // NOP padding
    ];
    setup_program(&mut bus, &mut cpu, &program);

    run_instruction(&mut cpu, &mut bus);
    run_instruction(&mut cpu, &mut bus);

    assert_eq!(
        cpu.pc(),
        0x0300,
        "BRK must use correct vector after INC abs"
    );
    assert_eq!(bus.peek(0x1234), 0x42, "INC should have incremented");
}

#[test]
fn test_brk_after_jmp_indirect() {
    // JMP ($1000) uses addr for the indirect lookup
    // This is tricky: JMP actually changes PC, so BRK is at a different location
    let mut bus = SimpleBus::new();
    let mut cpu = Mos6502::new();
    setup_brk_test(&mut bus, &mut cpu);

    // Pointer at $1000 points to $0210 (where BRK will be)
    bus.write(0x1000, 0x10);
    bus.write(0x1001, 0x02);

    // BRK at $0210
    bus.write(0x0210, 0x00); // BRK
    bus.write(0x0211, 0xEA); // NOP padding

    let program = [
        0x6C, 0x00, 0x10, // JMP ($1000) -> jumps to $0210
    ];
    setup_program(&mut bus, &mut cpu, &program);

    // JMP, then BRK
    run_instruction(&mut cpu, &mut bus);
    assert_eq!(cpu.pc(), 0x0210, "JMP should have jumped to $0210");

    run_instruction(&mut cpu, &mut bus);
    assert_eq!(
        cpu.pc(),
        0x0300,
        "BRK must use correct vector after JMP (ind)"
    );
}

#[test]
fn test_brk_after_page_crossing_read() {
    // LDA $10FF,X with X=$01 causes page crossing (addr goes through $1000 then $1100)
    let mut bus = SimpleBus::new();
    let mut cpu = Mos6502::new();
    setup_brk_test(&mut bus, &mut cpu);

    bus.write(0x1100, 0x42); // Correct address after page fix

    let program = [
        0xA2, 0x01, // LDX #$01
        0xBD, 0xFF, 0x10, // LDA $10FF,X (page cross: $10FF + 1 = $1100)
        0x00, // BRK
        0xEA, // NOP padding
    ];
    setup_program(&mut bus, &mut cpu, &program);

    for _ in 0..3 {
        run_instruction(&mut cpu, &mut bus);
    }

    assert_eq!(cpu.regs.a, 0x42, "LDA should have loaded from $1100");
    assert_eq!(
        cpu.pc(),
        0x0300,
        "BRK must use correct vector after page-crossing LDA"
    );
}

#[test]
fn test_brk_after_sta_absolute() {
    // STA $1234 uses addr for write target
    let mut bus = SimpleBus::new();
    let mut cpu = Mos6502::new();
    setup_brk_test(&mut bus, &mut cpu);

    let program = [
        0xA9, 0x42, // LDA #$42
        0x8D, 0x34, 0x12, // STA $1234
        0x00, // BRK
        0xEA, // NOP padding
    ];
    setup_program(&mut bus, &mut cpu, &program);

    for _ in 0..3 {
        run_instruction(&mut cpu, &mut bus);
    }

    assert_eq!(bus.peek(0x1234), 0x42, "STA should have stored");
    assert_eq!(
        cpu.pc(),
        0x0300,
        "BRK must use correct vector after STA abs"
    );
}

#[test]
fn test_brk_after_jsr_rts_sequence() {
    // JSR/RTS both use addr for return address manipulation
    let mut bus = SimpleBus::new();
    let mut cpu = Mos6502::new();
    setup_brk_test(&mut bus, &mut cpu);

    // Subroutine at $0220 that does LDA $4000 then RTS
    bus.write(0x0220, 0xAD); // LDA $4000
    bus.write(0x0221, 0x00);
    bus.write(0x0222, 0x40);
    bus.write(0x0223, 0x60); // RTS
    bus.write(0x4000, 0x42); // Data for LDA

    let program = [
        0x20, 0x20, 0x02, // JSR $0220
        0x00, // BRK (at $0203)
        0xEA, // NOP padding
    ];
    setup_program(&mut bus, &mut cpu, &program);

    // JSR
    run_instruction(&mut cpu, &mut bus);
    assert_eq!(cpu.pc(), 0x0220, "JSR should jump to subroutine");

    // LDA $4000
    run_instruction(&mut cpu, &mut bus);
    assert_eq!(cpu.regs.a, 0x42, "LDA in subroutine should load");

    // RTS
    run_instruction(&mut cpu, &mut bus);
    assert_eq!(cpu.pc(), 0x0203, "RTS should return after JSR");

    // BRK
    run_instruction(&mut cpu, &mut bus);
    assert_eq!(
        cpu.pc(),
        0x0300,
        "BRK must use correct vector after JSR/RTS sequence"
    );
}

// ============================================================================
// Illegal opcode tests
// ============================================================================

#[test]
fn test_illegal_lax_zeropage() {
    let mut bus = SimpleBus::new();
    let mut cpu = Mos6502::new();

    // Store $42 at zero page $10
    bus.write(0x0010, 0x42);

    // LAX $10 (opcode $A7)
    let program = [0xA7, 0x10];
    setup_program(&mut bus, &mut cpu, &program);
    run_instruction(&mut cpu, &mut bus);

    assert_eq!(cpu.regs.a, 0x42, "LAX should load A");
    assert_eq!(cpu.regs.x, 0x42, "LAX should load X with same value");
    assert!(
        !cpu.regs.p.is_set(flags::Z),
        "Z should be clear for non-zero"
    );
    assert!(!cpu.regs.p.is_set(flags::N), "N should be clear for $42");
}

#[test]
fn test_illegal_sax_zeropage() {
    let mut bus = SimpleBus::new();
    let mut cpu = Mos6502::new();

    // LDA #$0F; LDX #$F0; SAX $10
    let program = [
        0xA9, 0x0F, // LDA #$0F
        0xA2, 0xF0, // LDX #$F0
        0x87, 0x10, // SAX $10
    ];
    setup_program(&mut bus, &mut cpu, &program);

    run_instruction(&mut cpu, &mut bus); // LDA
    run_instruction(&mut cpu, &mut bus); // LDX
    run_instruction(&mut cpu, &mut bus); // SAX

    // A AND X = $0F AND $F0 = $00
    assert_eq!(bus.peek(0x0010), 0x00, "SAX should store A AND X");
}

#[test]
fn test_illegal_slo_zeropage() {
    let mut bus = SimpleBus::new();
    let mut cpu = Mos6502::new();

    // Store $40 at zero page $10
    bus.write(0x0010, 0x40);

    // LDA #$01; SLO $10
    let program = [
        0xA9, 0x01, // LDA #$01
        0x07, 0x10, // SLO $10
    ];
    setup_program(&mut bus, &mut cpu, &program);

    run_instruction(&mut cpu, &mut bus); // LDA
    run_instruction(&mut cpu, &mut bus); // SLO

    // $40 << 1 = $80, then A = $01 | $80 = $81
    assert_eq!(bus.peek(0x0010), 0x80, "SLO should shift memory left");
    assert_eq!(cpu.regs.a, 0x81, "SLO should OR result with A");
    assert!(
        !cpu.regs.p.is_set(flags::C),
        "C should be clear (bit 7 of $40 is 0)"
    );
    assert!(cpu.regs.p.is_set(flags::N), "N should be set for $81");
}

#[test]
fn test_illegal_rla_zeropage() {
    let mut bus = SimpleBus::new();
    let mut cpu = Mos6502::new();

    // Store $80 at zero page $10, set carry
    bus.write(0x0010, 0x80);

    // SEC; LDA #$FF; RLA $10
    let program = [
        0x38, // SEC
        0xA9, 0xFF, // LDA #$FF
        0x27, 0x10, // RLA $10
    ];
    setup_program(&mut bus, &mut cpu, &program);

    run_instruction(&mut cpu, &mut bus); // SEC
    run_instruction(&mut cpu, &mut bus); // LDA
    run_instruction(&mut cpu, &mut bus); // RLA

    // ROL($80) with C=1 = $01 (C becomes 1)
    // A = $FF AND $01 = $01
    assert_eq!(bus.peek(0x0010), 0x01, "RLA should rotate memory left");
    assert_eq!(cpu.regs.a, 0x01, "RLA should AND result with A");
    assert!(
        cpu.regs.p.is_set(flags::C),
        "C should be set from bit 7 of $80"
    );
}

#[test]
fn test_illegal_dcp_zeropage() {
    let mut bus = SimpleBus::new();
    let mut cpu = Mos6502::new();

    // Store $42 at zero page $10
    bus.write(0x0010, 0x42);

    // LDA #$41; DCP $10
    let program = [
        0xA9, 0x41, // LDA #$41
        0xC7, 0x10, // DCP $10
    ];
    setup_program(&mut bus, &mut cpu, &program);

    run_instruction(&mut cpu, &mut bus); // LDA
    run_instruction(&mut cpu, &mut bus); // DCP

    // DEC($42) = $41, then CMP A($41) with $41
    assert_eq!(bus.peek(0x0010), 0x41, "DCP should decrement memory");
    assert!(cpu.regs.p.is_set(flags::Z), "Z should be set (A == M)");
    assert!(cpu.regs.p.is_set(flags::C), "C should be set (A >= M)");
}

#[test]
fn test_illegal_isc_zeropage() {
    let mut bus = SimpleBus::new();
    let mut cpu = Mos6502::new();

    // Store $41 at zero page $10
    bus.write(0x0010, 0x41);

    // SEC; LDA #$43; ISC $10
    let program = [
        0x38, // SEC (no borrow)
        0xA9, 0x43, // LDA #$43
        0xE7, 0x10, // ISC $10
    ];
    setup_program(&mut bus, &mut cpu, &program);

    run_instruction(&mut cpu, &mut bus); // SEC
    run_instruction(&mut cpu, &mut bus); // LDA
    run_instruction(&mut cpu, &mut bus); // ISC

    // INC($41) = $42, then A = $43 - $42 = $01
    assert_eq!(bus.peek(0x0010), 0x42, "ISC should increment memory");
    assert_eq!(cpu.regs.a, 0x01, "ISC should subtract result from A");
    assert!(cpu.regs.p.is_set(flags::C), "C should be set (no borrow)");
}

#[test]
fn test_illegal_anc_immediate() {
    let mut bus = SimpleBus::new();
    let mut cpu = Mos6502::new();

    // LDA #$80; ANC #$FF
    let program = [
        0xA9, 0x80, // LDA #$80
        0x0B, 0xFF, // ANC #$FF
    ];
    setup_program(&mut bus, &mut cpu, &program);

    run_instruction(&mut cpu, &mut bus); // LDA
    run_instruction(&mut cpu, &mut bus); // ANC

    // A = $80 AND $FF = $80, C = N = 1
    assert_eq!(cpu.regs.a, 0x80, "ANC should AND");
    assert!(cpu.regs.p.is_set(flags::N), "N should be set for $80");
    assert!(cpu.regs.p.is_set(flags::C), "C should copy N flag");
}

#[test]
fn test_illegal_alr_immediate() {
    let mut bus = SimpleBus::new();
    let mut cpu = Mos6502::new();

    // LDA #$FF; ALR #$0F
    let program = [
        0xA9, 0xFF, // LDA #$FF
        0x4B, 0x0F, // ALR #$0F
    ];
    setup_program(&mut bus, &mut cpu, &program);

    run_instruction(&mut cpu, &mut bus); // LDA
    run_instruction(&mut cpu, &mut bus); // ALR

    // A = ($FF AND $0F) >> 1 = $0F >> 1 = $07
    assert_eq!(cpu.regs.a, 0x07, "ALR should AND then LSR");
    assert!(
        cpu.regs.p.is_set(flags::C),
        "C should be set (bit 0 of $0F)"
    );
    assert!(!cpu.regs.p.is_set(flags::N), "N should be clear");
}

#[test]
fn test_illegal_axs_immediate() {
    let mut bus = SimpleBus::new();
    let mut cpu = Mos6502::new();

    // LDA #$0F; LDX #$F0; AXS #$00
    let program = [
        0xA9, 0x0F, // LDA #$0F
        0xA2, 0xF0, // LDX #$F0
        0xCB, 0x00, // AXS #$00
    ];
    setup_program(&mut bus, &mut cpu, &program);

    run_instruction(&mut cpu, &mut bus); // LDA
    run_instruction(&mut cpu, &mut bus); // LDX
    run_instruction(&mut cpu, &mut bus); // AXS

    // X = (A AND X) - imm = ($0F AND $F0) - $00 = $00 - $00 = $00
    assert_eq!(cpu.regs.x, 0x00, "AXS should compute (A AND X) - imm");
    assert!(cpu.regs.p.is_set(flags::Z), "Z should be set for $00");
    assert!(cpu.regs.p.is_set(flags::C), "C should be set (no borrow)");
}

#[test]
fn test_illegal_nop_single_byte() {
    let mut bus = SimpleBus::new();
    let mut cpu = Mos6502::new();

    // $1A is a single-byte NOP
    let program = [0x1A, 0xA9, 0x42]; // NOP, LDA #$42
    setup_program(&mut bus, &mut cpu, &program);

    let start_pc = cpu.pc();
    run_instruction(&mut cpu, &mut bus); // NOP

    assert_eq!(
        cpu.pc(),
        start_pc + 1,
        "Single-byte NOP should advance PC by 1"
    );

    run_instruction(&mut cpu, &mut bus); // LDA
    assert_eq!(cpu.regs.a, 0x42, "Next instruction should execute normally");
}

#[test]
fn test_illegal_nop_two_byte() {
    let mut bus = SimpleBus::new();
    let mut cpu = Mos6502::new();

    // $80 is a two-byte NOP (immediate)
    let program = [0x80, 0xFF, 0xA9, 0x42]; // NOP #$FF, LDA #$42
    setup_program(&mut bus, &mut cpu, &program);

    let start_pc = cpu.pc();
    run_instruction(&mut cpu, &mut bus); // NOP

    assert_eq!(
        cpu.pc(),
        start_pc + 2,
        "Two-byte NOP should advance PC by 2"
    );

    run_instruction(&mut cpu, &mut bus); // LDA
    assert_eq!(cpu.regs.a, 0x42, "Next instruction should execute normally");
}

#[test]
fn test_illegal_nop_three_byte() {
    let mut bus = SimpleBus::new();
    let mut cpu = Mos6502::new();

    // $0C is a three-byte NOP (absolute)
    let program = [0x0C, 0x00, 0x10, 0xA9, 0x42]; // NOP $1000, LDA #$42
    setup_program(&mut bus, &mut cpu, &program);

    let start_pc = cpu.pc();
    run_instruction(&mut cpu, &mut bus); // NOP

    assert_eq!(
        cpu.pc(),
        start_pc + 3,
        "Three-byte NOP should advance PC by 3"
    );

    run_instruction(&mut cpu, &mut bus); // LDA
    assert_eq!(cpu.regs.a, 0x42, "Next instruction should execute normally");
}

#[test]
fn test_illegal_jam_halts_cpu() {
    let mut bus = SimpleBus::new();
    let mut cpu = Mos6502::new();

    // $02 is a JAM/KIL opcode
    let program = [0x02, 0xA9, 0x42]; // JAM, LDA #$42
    setup_program(&mut bus, &mut cpu, &program);

    // JAM halts the CPU - it never "completes" in the normal sense
    // Just tick once to execute the JAM
    cpu.tick(&mut bus); // Fetch JAM opcode
    cpu.tick(&mut bus); // Execute - enters Stopped state

    // CPU should be stopped - further ticks shouldn't change PC
    let pc_before = cpu.pc();
    for _ in 0..10 {
        cpu.tick(&mut bus);
    }

    // PC should stay the same because CPU is halted
    assert_eq!(cpu.pc(), pc_before, "JAM should halt the CPU");
    assert_ne!(cpu.regs.a, 0x42, "LDA should not have executed");
}
