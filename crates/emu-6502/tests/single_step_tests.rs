//! Integration tests using Tom Harte's `SingleStepTests` for the 6502.
//!
//! Runs 256 opcode files x 10,000 tests = 2,560,000 individual tests comparing
//! CPU register and memory state after each instruction.
//!
//! Test data lives in `test-data/65x02/6502/v1/XX.json`.

use emu_6502::Mos6502;
use emu_core::{Bus, Cpu, ReadResult};
use serde::Deserialize;
use std::fs;
use std::path::Path;

/// Flat 64KB RAM bus for testing.
struct TestBus {
    ram: [u8; 65536],
}

impl TestBus {
    #[allow(clippy::large_stack_arrays)]
    fn new() -> Self {
        Self { ram: [0; 65536] }
    }

    fn load_ram(&mut self, entries: &[(u16, u8)]) {
        for &(addr, value) in entries {
            self.ram[addr as usize] = value;
        }
    }

    fn peek(&self, addr: u16) -> u8 {
        self.ram[addr as usize]
    }
}

impl Bus for TestBus {
    fn read(&mut self, addr: u32) -> ReadResult {
        ReadResult::new(self.ram[(addr & 0xFFFF) as usize])
    }

    fn write(&mut self, addr: u32, value: u8) -> u8 {
        self.ram[(addr & 0xFFFF) as usize] = value;
        0
    }

    fn io_read(&mut self, _addr: u32) -> ReadResult {
        ReadResult::new(0xFF)
    }

    fn io_write(&mut self, _addr: u32, _value: u8) -> u8 {
        0
    }
}

/// JSON test case format.
#[derive(Deserialize)]
struct TestCase {
    name: String,
    initial: CpuState,
    #[serde(rename = "final")]
    final_state: CpuState,
    cycles: Vec<(u16, u8, String)>,
}

/// JSON CPU state format.
#[derive(Deserialize)]
struct CpuState {
    pc: u16,
    s: u8,
    a: u8,
    x: u8,
    y: u8,
    p: u8,
    ram: Vec<(u16, u8)>,
}

/// Set up the CPU and bus from the initial test state.
fn setup(cpu: &mut Mos6502, bus: &mut TestBus, state: &CpuState) {
    bus.load_ram(&state.ram);
    cpu.regs.pc = state.pc;
    cpu.regs.s = state.s;
    cpu.regs.a = state.a;
    cpu.regs.x = state.x;
    cpu.regs.y = state.y;
    cpu.regs.p = emu_6502::Status::from_byte(state.p);
}

/// Compare the CPU/bus state against expected, returning a list of mismatches.
fn compare(cpu: &Mos6502, bus: &TestBus, expected: &CpuState) -> Vec<String> {
    let mut errors = Vec::new();

    if cpu.regs.pc != expected.pc {
        errors.push(format!(
            "PC: got ${:04X}, want ${:04X}",
            cpu.regs.pc, expected.pc
        ));
    }
    if cpu.regs.s != expected.s {
        errors.push(format!(
            "S: got ${:02X}, want ${:02X}",
            cpu.regs.s, expected.s
        ));
    }
    if cpu.regs.a != expected.a {
        errors.push(format!(
            "A: got ${:02X}, want ${:02X}",
            cpu.regs.a, expected.a
        ));
    }
    if cpu.regs.x != expected.x {
        errors.push(format!(
            "X: got ${:02X}, want ${:02X}",
            cpu.regs.x, expected.x
        ));
    }
    if cpu.regs.y != expected.y {
        errors.push(format!(
            "Y: got ${:02X}, want ${:02X}",
            cpu.regs.y, expected.y
        ));
    }

    // Compare raw P register value. Status::from_byte() already forces U=1.
    // We use .0 (raw internal bits) rather than to_byte() which strips B.
    // The B flag only matters when P is pushed to the stack, not internally.
    let actual_p = cpu.regs.p.0;
    let expected_p = expected.p | 0x20;
    if actual_p != expected_p {
        errors.push(format!(
            "P: got ${actual_p:02X} ({actual_p:08b}), want ${expected_p:02X} ({expected_p:08b})"
        ));
    }

    for &(addr, expected_val) in &expected.ram {
        let actual_val = bus.peek(addr);
        if actual_val != expected_val {
            errors.push(format!(
                "RAM[${addr:04X}]: got ${actual_val:02X}, want ${expected_val:02X}"
            ));
        }
    }

    errors
}

#[test]
#[ignore = "requires test-data/65x02 — run with --ignored"]
fn run_all() {
    let test_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("parent of crate dir")
        .parent()
        .expect("workspace root")
        .join("test-data/65x02/6502/v1");

    if !test_dir.exists() {
        eprintln!("Test data not found at {}", test_dir.display());
        eprintln!("Skipping SingleStepTests.");
        return;
    }

    let mut total_pass = 0u64;
    let mut total_fail = 0u64;
    let mut total_files = 0u32;

    for opcode in 0..=0xFF_u8 {
        let filename = format!("{opcode:02x}.json");
        let path = test_dir.join(&filename);
        if !path.exists() {
            continue;
        }

        let data = fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!("Failed to read {}: {e}", path.display());
        });
        let tests: Vec<TestCase> = serde_json::from_str(&data).unwrap_or_else(|e| {
            panic!("Failed to parse {}: {e}", path.display());
        });

        let mut file_pass = 0u32;
        let mut file_fail = 0u32;
        let mut first_failures: Vec<String> = Vec::new();

        for test in &tests {
            let mut cpu = Mos6502::new();
            let mut bus = TestBus::new();

            setup(&mut cpu, &mut bus, &test.initial);

            let expected_ticks = test.cycles.len();
            for _ in 0..expected_ticks {
                cpu.tick(&mut bus);
            }

            let errors = compare(&cpu, &bus, &test.final_state);
            if errors.is_empty() {
                file_pass += 1;
            } else {
                file_fail += 1;
                if first_failures.len() < 5 {
                    first_failures.push(format!(
                        "  FAIL [{}]: {}",
                        test.name,
                        errors.join(", ")
                    ));
                }
            }
        }

        let status = if file_fail == 0 { "PASS" } else { "FAIL" };
        println!(
            "Opcode ${opcode:02X} ({filename}): {status} — {file_pass}/{} passed",
            file_pass + file_fail
        );
        for msg in &first_failures {
            println!("{msg}");
        }

        total_pass += u64::from(file_pass);
        total_fail += u64::from(file_fail);
        total_files += 1;
    }

    println!();
    println!("=== SingleStepTests Summary ===");
    println!(
        "Files: {total_files}, Total: {}/{}, Pass: {total_pass}, Fail: {total_fail}",
        total_pass + total_fail,
        total_pass + total_fail
    );

    assert_eq!(total_fail, 0, "{total_fail} tests failed");
}
