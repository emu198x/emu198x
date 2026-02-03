//! Klaus Dormann's 6502 functional test harness.
//!
//! The functional test exercises all documented 6502 opcodes.
//! Test binary should be assembled with load address $0000.
//!
//! Test structure:
//! - $0400: Test entry point
//! - Test completes when PC gets stuck (trap - branches to itself)
//! - Success: PC reaches $3469
//! - Failure: PC reaches any other trap address

use emu_core::{Cpu, SimpleBus};
use emu_6502::Mos6502;

/// Run the Klaus Dormann 6502 functional test.
fn run_dormann(binary: &[u8]) -> bool {
    let mut bus = SimpleBus::new();

    // Load test binary at $0000
    bus.load(0x0000, binary);

    let mut cpu = Mos6502::new();

    // Start execution at $0400 (test entry point)
    cpu.regs.pc = 0x0400;

    let mut cycles: u64 = 0;
    let mut instructions: u64 = 0;
    let mut last_good_pc: u16 = 0x0400;

    let mut prev_pc: u16 = 0xFFFF;
    let mut same_pc_count = 0;

    loop {
        let start_pc = cpu.pc();

        // Detect trap: PC didn't change (branch to self)
        if start_pc == prev_pc {
            same_pc_count += 1;
            if same_pc_count > 2 {
                eprintln!("\nTrapped at ${:04X} after {} instructions ({} cycles)",
                         start_pc, instructions, cycles);
                // Success address for the standard test
                return start_pc == 0x3469;
            }
        } else {
            same_pc_count = 0;
            prev_pc = start_pc;
        }

        // Detect if we've jumped to $FF00+ region (usually indicates bad vector read)
        if start_pc >= 0xFF00 && last_good_pc < 0xFF00 {
            eprintln!("\n!!! Jumped to ${:04X} from ${:04X} after {} instructions",
                     start_pc, last_good_pc, instructions);
            return false;
        }

        if start_pc < 0xFF00 {
            last_good_pc = start_pc;
        }

        // Run one instruction - first tick does fetch
        cpu.tick(&mut bus);
        cycles += 1;

        // Continue until instruction completes (back to FetchOpcode state)
        loop {
            if cpu.is_instruction_complete() {
                break;
            }
            cpu.tick(&mut bus);
            cycles += 1;
        }

        instructions += 1;

        // Progress every 100K instructions
        if instructions % 100_000 == 0 {
            eprint!("\r[{} instructions, PC=${:04X}]", instructions, cpu.pc());
        }

        // Safety limit
        if instructions > 100_000_000 {
            eprintln!("\nTest exceeded 100M instructions limit");
            return false;
        }
    }
}

/// Run the decimal mode test.
fn run_decimal_test(binary: &[u8]) -> bool {
    let mut bus = SimpleBus::new();

    // Load test binary at $0000
    bus.load(0x0000, binary);

    let mut cpu = Mos6502::new();

    // Start at $0200 for decimal test
    cpu.regs.pc = 0x0200;

    let mut cycles: u64 = 0;
    let mut instructions: u64 = 0;

    let mut prev_pc: u16 = 0xFFFF;
    let mut same_pc_count = 0;

    // Zero-page layout from test:
    // $00=N1, $01=N2, $02=HA, $03=HNVZC, $04=DA, $05=DNVZC
    // $06=AR, $07=NF, $08=VF, $09=ZF, $0A=CF, $0B=ERROR

    loop {
        let start_pc = cpu.pc();

        // Detect trap: PC didn't change between instructions (branch to self)
        if start_pc == prev_pc {
            same_pc_count += 1;
            if same_pc_count > 2 {
                eprintln!("\nTrapped at ${:04X} after {} instructions ({} cycles)",
                         start_pc, instructions, cycles);
                // Check error flag location
                let error = bus.peek(0x000B);
                eprintln!("Error flag at $000B: ${:02X}", error);

                // Dump test state on failure
                if error != 0 {
                    let n1 = bus.peek(0x00);
                    let n2 = bus.peek(0x01);
                    let da = bus.peek(0x04);    // Actual decimal result
                    let dnvzc = bus.peek(0x05); // Actual flags
                    let ar = bus.peek(0x06);    // Predicted accumulator
                    let cf = bus.peek(0x0A);    // Predicted carry
                    let y_reg = cpu.regs.y;     // Carry input (Y=1 means carry set)

                    eprintln!("Test state at failure:");
                    eprintln!("  N1=${:02X}, N2=${:02X}, Y(carry_in)={}", n1, n2, y_reg);
                    eprintln!("  Actual: A=${:02X}, Flags=${:02X}", da, dnvzc);
                    eprintln!("  Predicted: A=${:02X}, C_flag=${:02X}", ar, cf);

                    // Decode flags
                    let actual_c = dnvzc & 1;
                    let pred_c = cf & 1;
                    eprintln!("  Carry: actual={}, predicted={}", actual_c, pred_c);

                    if da != ar {
                        eprintln!("  >>> ACCUMULATOR MISMATCH <<<");
                    }
                    if actual_c != pred_c {
                        eprintln!("  >>> CARRY FLAG MISMATCH <<<");
                    }
                }

                return error == 0;
            }
        } else {
            same_pc_count = 0;
            prev_pc = start_pc;
        }

        // Run one instruction - first tick does fetch
        cpu.tick(&mut bus);
        cycles += 1;

        // Continue until instruction completes (back to FetchOpcode state)
        loop {
            if cpu.is_instruction_complete() {
                break;
            }
            cpu.tick(&mut bus);
            cycles += 1;
        }

        instructions += 1;

        // Progress every 100K instructions
        if instructions % 100_000 == 0 {
            eprint!("\r[{} instructions, PC=${:04X}]", instructions, cpu.pc());
        }

        // Safety limit
        if instructions > 50_000_000 {
            eprintln!("\nDecimal test exceeded 50M instructions limit");
            return false;
        }
    }
}

#[test]
#[ignore]
fn dormann_functional() {
    let binary = std::fs::read("tests/data/6502_functional_test.bin")
        .expect("tests/data/6502_functional_test.bin not found - download from Klaus Dormann's repository");
    assert!(run_dormann(&binary), "Klaus Dormann 6502 functional test failed");
}

#[test]
#[ignore]
fn dormann_decimal() {
    let binary = std::fs::read("tests/data/6502_decimal_test.bin")
        .expect("tests/data/6502_decimal_test.bin not found");
    assert!(run_decimal_test(&binary), "Klaus Dormann decimal test failed");
}
