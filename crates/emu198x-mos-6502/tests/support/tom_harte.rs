//! The Tom Harte single-step runner, shared by the NMOS and CMOS suites.
//!
//! Each case loads one instruction into a flat 64 KB of RAM, checks the bus
//! on every cycle against the recorded trace, and compares the final state.

use std::path::Path;

use emu198x_mos_6502::M6502;
use serde::Deserialize;

/// The JSON opcode file for `opcode` under `root`.
pub fn opcode_path(root: &Path, opcode: u8) -> std::path::PathBuf {
    root.join(format!("{opcode:02x}.json"))
}

#[derive(Deserialize)]
pub struct TestCase {
    name: String,
    initial: State,
    #[serde(rename = "final")]
    final_state: State,
    cycles: Vec<(u16, u8, String)>,
}

#[derive(Deserialize)]
pub struct State {
    pc: u16,
    s: u8,
    a: u8,
    x: u8,
    y: u8,
    p: u8,
    ram: Vec<(u16, u8)>,
}

fn setup_cpu(cpu: &mut M6502, state: &State, mem: &[u8; 65536]) {
    cpu.regs.pc = state.pc;
    cpu.regs.sp = state.s;
    cpu.regs.a = state.a;
    cpu.regs.x = state.x;
    cpu.regs.y = state.y;
    cpu.regs.p = state.p;
    cpu.total_cycles = 0;
    cpu.addr = state.pc;
    cpu.data = 0;
    cpu.rw = true;
    cpu.sync = true;
    cpu.data_in = mem[state.pc as usize];
    cpu.irq = false;
    cpu.nmi = false;
    cpu.rdy = true;
    cpu.halted = false;
}

fn check_cpu(cpu: &M6502, expected: &State, mem: &[u8; 65536]) -> Vec<String> {
    let mut errors = Vec::new();

    macro_rules! check {
        ($name:expr, $got:expr, $exp:expr) => {
            if $got != $exp {
                errors.push(format!(
                    "{}: got {:#06X}, expected {:#06X}",
                    $name, $got, $exp
                ));
            }
        };
    }

    check!("PC", cpu.regs.pc, expected.pc);
    check!("S", cpu.regs.sp as u16, expected.s as u16);
    check!("A", cpu.regs.a as u16, expected.a as u16);
    check!("X", cpu.regs.x as u16, expected.x as u16);
    check!("Y", cpu.regs.y as u16, expected.y as u16);
    check!("P", cpu.regs.p as u16, expected.p as u16);

    for &(addr, val) in &expected.ram {
        if mem[addr as usize] != val {
            errors.push(format!(
                "RAM[{:#06X}]: got {:#04X}, expected {:#04X}",
                addr, mem[addr as usize], val
            ));
        }
    }

    errors
}

pub fn run_test(test: &TestCase, new_cpu: fn() -> M6502) -> Vec<String> {
    let mut cpu = new_cpu();
    let mut mem = [0u8; 65536];

    for &(addr, val) in &test.initial.ram {
        mem[addr as usize] = val;
    }
    for &(addr, data, ref kind) in &test.cycles {
        if kind != "write" {
            mem[addr as usize] = data;
        }
    }
    for &(addr, val) in &test.initial.ram {
        mem[addr as usize] = val;
    }

    setup_cpu(&mut cpu, &test.initial, &mem);

    let mut errors = Vec::new();
    for (i, &(exp_addr, exp_data, ref kind)) in test.cycles.iter().enumerate() {
        // The CPU presents cycle i's bus operation on its pins before the
        // machine services it. Assert the address, direction, and (on writes)
        // the data driven match Tom Harte's per-cycle trace — the bus-accuracy
        // dimension the final register/RAM comparison is blind to (dummy-read
        // address, RMW double-write target, JMP-indirect fetch order, etc.).
        let want_read = kind != "write";
        if cpu.addr != exp_addr {
            errors.push(format!(
                "cycle{i}.addr: got {:#06X}, expected {exp_addr:#06X}",
                cpu.addr
            ));
        }
        if cpu.rw != want_read {
            errors.push(format!(
                "cycle{i}.rw: got {}, expected {kind}",
                if cpu.rw { "read" } else { "write" }
            ));
        }

        if cpu.rw {
            cpu.data_in = mem[cpu.addr as usize];
        } else {
            if cpu.data != exp_data {
                errors.push(format!(
                    "cycle{i}.data: got {:#04X}, expected {exp_data:#04X}",
                    cpu.data
                ));
            }
            mem[cpu.addr as usize] = cpu.data;
        }
        cpu.tick();
    }

    errors.extend(check_cpu(&cpu, &test.final_state, &mem));
    errors
}

/// Run every case in one opcode file on a fresh CPU from `new_cpu`, passing
/// each case's errors through `filter` (given the opcode) before counting it.
pub fn run_opcode_tests(
    path: &Path,
    new_cpu: fn() -> M6502,
    filter: fn(u8, Vec<String>) -> Vec<String>,
) -> (usize, usize, Vec<String>) {
    let data = std::fs::read_to_string(path).expect("failed to read JSON opcode file");
    let tests: Vec<TestCase> =
        serde_json::from_str(&data).expect("failed to parse JSON opcode file");
    // Derive the opcode from the filename (e.g. "ab.json" → 0xAB)
    // for the per-opcode allowlist lookup.
    let opcode = path
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| u8::from_str_radix(s, 16).ok())
        .unwrap_or(0);

    let mut pass = 0usize;
    let mut fail = 0usize;
    let mut first_failures = Vec::new();

    for test in &tests {
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_test(test, new_cpu)));
        match result {
            Ok(errors) => {
                let errors = filter(opcode, errors);
                if errors.is_empty() {
                    pass += 1;
                } else {
                    fail += 1;
                    if first_failures.len() < 3 {
                        first_failures.push(format!("FAIL {}: {}", test.name, errors.join(", ")));
                    }
                }
            }
            Err(_) => {
                fail += 1;
                if first_failures.len() < 3 {
                    first_failures.push(format!("PANIC {}", test.name));
                }
            }
        }
    }

    (pass, fail, first_failures)
}
