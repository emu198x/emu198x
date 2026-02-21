//! Minimal CP/M harness for ZEXDOC/ZEXALL.
//!
//! CP/M memory layout:
//! - 0x0000: Warm boot (JP to BIOS, we use HALT)
//! - 0x0005: BDOS entry (we intercept CALL 5)
//! - 0x0006-0x0007: Top of TPA (programs read this for stack init)
//! - 0x0100: Program load address (TPA start)

use std::io::Write;
use emu_core::{Cpu, SimpleBus};
use zilog_z80::{MicroOp, Z80};

fn run_zex(binary: &[u8]) -> bool {
    let mut bus = SimpleBus::new();

    // Load program at 0x0100
    bus.load(0x0100, binary);

    // Warm boot at 0x0000 - HALT to signal exit
    bus.load(0x0000, &[0x76]); // HALT

    // BDOS entry at 0x0005 - RET (we intercept before execution)
    bus.load(0x0005, &[0xC9]); // RET

    // Top of TPA at 0x0006-0x0007 (little-endian)
    // Programs do: LD HL,(0006) / LD SP,HL
    bus.load(0x0006, &[0x00, 0xFE]); // 0xFE00

    let mut cpu = Z80::new();
    cpu.set_pc(0x0100);

    let mut output = String::new();
    let mut instructions: u64 = 0;

    loop {
        let pc = cpu.pc();

        // Check for instruction boundary
        let at_instruction_start = cpu.current_micro_op() == Some(MicroOp::FetchOpcode)
            && cpu.t_state() == 0;

        if at_instruction_start {
            instructions += 1;

            // Progress every 1M instructions
            if instructions % 1_000_000 == 0 {
                eprintln!("[{} instructions]", instructions);
            }
        }

        // Exit on warm boot (PC=0x0000)
        if pc == 0x0000 && at_instruction_start {
            eprintln!("Warm boot at instruction {}", instructions);
            break;
        }

        // Exit on HALT
        if cpu.is_halted() {
            eprintln!("HALT at instruction {}", instructions);
            break;
        }

        // BDOS intercept at 0x0005
        if pc == 0x0005 && at_instruction_start {
            let func = cpu.c();
            match func {
                2 => {
                    // Print character in E
                    let ch = cpu.e() as char;
                    eprint!("{}", ch);
                    std::io::stderr().flush().unwrap();
                    output.push(ch);
                }
                9 => {
                    // Print string at DE until '$'
                    let mut addr = cpu.de();
                    loop {
                        let ch = bus.peek(addr);
                        if ch == b'$' {
                            break;
                        }
                        eprint!("{}", ch as char);
                        output.push(ch as char);
                        addr = addr.wrapping_add(1);
                    }
                    std::io::stderr().flush().unwrap();
                }
                _ => {
                    eprintln!("\nUnknown BDOS function: {}", func);
                }
            }
            // Simulate RET - pop return address from stack
            cpu.ret(&mut bus);
            continue;
        }

        cpu.tick(&mut bus);
    }

    eprintln!("\nTotal: {} instructions", instructions);
    eprintln!("Output length: {} chars", output.len());

    // ZEXDOC outputs "ERROR" on failure
    !output.contains("ERROR")
}

#[test]
#[ignore]
fn zexdoc() {
    let binary = std::fs::read("tests/data/zexdoc.com")
        .expect("tests/data/zexdoc.com not found");
    assert!(run_zex(&binary), "ZEXDOC failed");
}

#[test]
#[ignore]
fn zexall() {
    let binary = std::fs::read("tests/data/zexall.com")
        .expect("tests/data/zexall.com not found");
    assert!(run_zex(&binary), "ZEXALL failed");
}
