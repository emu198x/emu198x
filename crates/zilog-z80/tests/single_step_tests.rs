//! Integration tests using Tom Harte's `SingleStepTests` for the Z80.
//!
//! Runs 1,604 opcode files × 1,000 tests = 1,604,000 individual tests comparing
//! CPU register and memory state after each instruction.
//!
//! Test data lives in `test-data/z80/v1/`.

use emu_core::{Bus, Cpu, ReadResult};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::panic;
use std::path::Path;
use zilog_z80::Z80;

/// Flat 64KB RAM bus with I/O port support for testing.
struct TestBus {
    ram: [u8; 65536],
    /// Preloaded port values for IN instructions.
    io_read_values: HashMap<u16, u8>,
}

impl TestBus {
    #[allow(clippy::large_stack_arrays)]
    fn new() -> Self {
        Self {
            ram: [0; 65536],
            io_read_values: HashMap::new(),
        }
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

    fn io_read(&mut self, addr: u32) -> ReadResult {
        let port = (addr & 0xFFFF) as u16;
        let value = self.io_read_values.get(&port).copied().unwrap_or(0xFF);
        ReadResult::new(value)
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
    cycles: Vec<serde_json::Value>,
    #[serde(default)]
    ports: Vec<(u16, u8, String)>,
}

/// JSON CPU state format.
#[derive(Deserialize)]
struct CpuState {
    pc: u16,
    sp: u16,
    a: u8,
    b: u8,
    c: u8,
    d: u8,
    e: u8,
    f: u8,
    h: u8,
    l: u8,
    i: u8,
    r: u8,
    ix: u16,
    iy: u16,
    wz: u16,
    #[serde(rename = "af_")]
    af_alt: u16,
    #[serde(rename = "bc_")]
    bc_alt: u16,
    #[serde(rename = "de_")]
    de_alt: u16,
    #[serde(rename = "hl_")]
    hl_alt: u16,
    iff1: u8,
    iff2: u8,
    im: u8,
    ei: u8,
    p: u8,
    q: u8,
    ram: Vec<(u16, u8)>,
}

/// Set up the CPU and bus from the initial test state.
fn setup(cpu: &mut Z80, bus: &mut TestBus, state: &CpuState, ports: &[(u16, u8, String)]) {
    // Load RAM
    bus.load_ram(&state.ram);

    // Load I/O port read values
    bus.io_read_values.clear();
    for &(port, value, ref dir) in ports {
        if dir == "r" {
            bus.io_read_values.insert(port, value);
        }
    }

    // Main registers
    cpu.regs.a = state.a;
    cpu.regs.f = state.f;
    cpu.regs.b = state.b;
    cpu.regs.c = state.c;
    cpu.regs.d = state.d;
    cpu.regs.e = state.e;
    cpu.regs.h = state.h;
    cpu.regs.l = state.l;

    // Alternate registers (stored as 16-bit pairs)
    cpu.regs.a_alt = (state.af_alt >> 8) as u8;
    cpu.regs.f_alt = state.af_alt as u8;
    cpu.regs.b_alt = (state.bc_alt >> 8) as u8;
    cpu.regs.c_alt = state.bc_alt as u8;
    cpu.regs.d_alt = (state.de_alt >> 8) as u8;
    cpu.regs.e_alt = state.de_alt as u8;
    cpu.regs.h_alt = (state.hl_alt >> 8) as u8;
    cpu.regs.l_alt = state.hl_alt as u8;

    // Index registers
    cpu.regs.ix = state.ix;
    cpu.regs.iy = state.iy;

    // Other registers
    cpu.regs.sp = state.sp;
    cpu.regs.pc = state.pc;
    cpu.regs.i = state.i;
    cpu.regs.r = state.r;
    cpu.regs.wz = state.wz;

    // Interrupt state
    cpu.regs.iff1 = state.iff1 != 0;
    cpu.regs.iff2 = state.iff2 != 0;
    cpu.regs.im = state.im;

    // Internal state
    cpu.ei_delay = state.ei != 0;
    cpu.last_was_ld_a_ir = state.p != 0;
    cpu.last_q = state.q;
}

/// Compare the CPU/bus state against expected, returning a list of mismatches.
fn compare(cpu: &Z80, bus: &TestBus, expected: &CpuState) -> Vec<String> {
    let mut errors = Vec::new();

    // Main registers
    check_u8(&mut errors, "A", cpu.regs.a, expected.a);
    check_u8(&mut errors, "F", cpu.regs.f, expected.f);
    check_u8(&mut errors, "B", cpu.regs.b, expected.b);
    check_u8(&mut errors, "C", cpu.regs.c, expected.c);
    check_u8(&mut errors, "D", cpu.regs.d, expected.d);
    check_u8(&mut errors, "E", cpu.regs.e, expected.e);
    check_u8(&mut errors, "H", cpu.regs.h, expected.h);
    check_u8(&mut errors, "L", cpu.regs.l, expected.l);

    // Alternate registers
    let actual_af_alt = (u16::from(cpu.regs.a_alt) << 8) | u16::from(cpu.regs.f_alt);
    check_u16(&mut errors, "AF'", actual_af_alt, expected.af_alt);
    let actual_bc_alt = (u16::from(cpu.regs.b_alt) << 8) | u16::from(cpu.regs.c_alt);
    check_u16(&mut errors, "BC'", actual_bc_alt, expected.bc_alt);
    let actual_de_alt = (u16::from(cpu.regs.d_alt) << 8) | u16::from(cpu.regs.e_alt);
    check_u16(&mut errors, "DE'", actual_de_alt, expected.de_alt);
    let actual_hl_alt = (u16::from(cpu.regs.h_alt) << 8) | u16::from(cpu.regs.l_alt);
    check_u16(&mut errors, "HL'", actual_hl_alt, expected.hl_alt);

    // Index registers
    check_u16(&mut errors, "IX", cpu.regs.ix, expected.ix);
    check_u16(&mut errors, "IY", cpu.regs.iy, expected.iy);

    // Other registers
    check_u16(&mut errors, "SP", cpu.regs.sp, expected.sp);
    check_u16(&mut errors, "PC", cpu.regs.pc, expected.pc);
    check_u8(&mut errors, "I", cpu.regs.i, expected.i);
    check_u8(&mut errors, "R", cpu.regs.r, expected.r);

    // WZ/MEMPTR
    check_u16(&mut errors, "WZ", cpu.regs.wz, expected.wz);

    // Interrupt state
    let actual_iff1 = u8::from(cpu.regs.iff1);
    if actual_iff1 != expected.iff1 {
        errors.push(format!("IFF1: got {actual_iff1}, want {}", expected.iff1));
    }
    let actual_iff2 = u8::from(cpu.regs.iff2);
    if actual_iff2 != expected.iff2 {
        errors.push(format!("IFF2: got {actual_iff2}, want {}", expected.iff2));
    }
    check_u8(&mut errors, "IM", cpu.regs.im, expected.im);

    // Internal state
    let actual_ei = u8::from(cpu.ei_delay);
    if actual_ei != expected.ei {
        errors.push(format!("EI: got {actual_ei}, want {}", expected.ei));
    }
    let actual_p = u8::from(cpu.last_was_ld_a_ir);
    if actual_p != expected.p {
        errors.push(format!("P: got {actual_p}, want {}", expected.p));
    }
    if cpu.last_q != expected.q {
        errors.push(format!("Q: got {}, want {}", cpu.last_q, expected.q));
    }

    // RAM
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

fn check_u8(errors: &mut Vec<String>, name: &str, actual: u8, expected: u8) {
    if actual != expected {
        errors.push(format!("{name}: got ${actual:02X}, want ${expected:02X}"));
    }
}

fn check_u16(errors: &mut Vec<String>, name: &str, actual: u16, expected: u16) {
    if actual != expected {
        errors.push(format!("{name}: got ${actual:04X}, want ${expected:04X}"));
    }
}

/// Run all Z80 SingleStepTests.
///
/// Iterates through all 1,604 test files covering unprefixed, CB, DD, ED, and FD opcodes.
#[test]
#[ignore = "requires test-data/z80 — run with --ignored"]
fn run_all() {
    let test_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("parent of crate dir")
        .parent()
        .expect("workspace root")
        .join("test-data/z80/v1");

    if !test_dir.exists() {
        eprintln!("Test data not found at {}", test_dir.display());
        eprintln!("Skipping SingleStepTests.");
        return;
    }

    let mut total_pass = 0u64;
    let mut total_fail = 0u64;
    let mut total_files = 0u32;

    // Collect all filenames to test
    let mut filenames: Vec<String> = Vec::new();

    // Base opcodes (skip CB, DD, ED, FD prefix bytes)
    for opcode in 0..=0xFFu8 {
        if matches!(opcode, 0xCB | 0xDD | 0xED | 0xFD) {
            continue;
        }
        filenames.push(format!("{opcode:02x}.json"));
    }

    // CB-prefixed
    for opcode in 0..=0xFFu8 {
        filenames.push(format!("cb {opcode:02x}.json"));
    }

    // DD-prefixed
    for opcode in 0..=0xFFu8 {
        filenames.push(format!("dd {opcode:02x}.json"));
    }

    // ED-prefixed
    for opcode in 0..=0xFFu8 {
        filenames.push(format!("ed {opcode:02x}.json"));
    }

    // FD-prefixed
    for opcode in 0..=0xFFu8 {
        filenames.push(format!("fd {opcode:02x}.json"));
    }

    // DD CB-prefixed (displacement is __, opcode varies)
    for opcode in 0..=0xFFu8 {
        filenames.push(format!("dd cb __ {opcode:02x}.json"));
    }

    // FD CB-prefixed
    for opcode in 0..=0xFFu8 {
        filenames.push(format!("fd cb __ {opcode:02x}.json"));
    }

    for filename in &filenames {
        let path = test_dir.join(filename);
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
            let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                let mut cpu = Z80::new();
                let mut bus = TestBus::new();

                setup(&mut cpu, &mut bus, &test.initial, &test.ports);

                let expected_ticks = test.cycles.len();
                for _ in 0..expected_ticks {
                    cpu.tick(&mut bus);
                }

                compare(&cpu, &bus, &test.final_state)
            }));

            match result {
                Ok(errors) if errors.is_empty() => {
                    file_pass += 1;
                }
                Ok(errors) => {
                    file_fail += 1;
                    if first_failures.len() < 5 {
                        first_failures.push(format!(
                            "  FAIL [{}]: {}",
                            test.name,
                            errors.join(", ")
                        ));
                    }
                }
                Err(_) => {
                    file_fail += 1;
                    if first_failures.len() < 5 {
                        first_failures
                            .push(format!("  PANIC [{}]: unimplemented or crash", test.name,));
                    }
                }
            }
        }

        let status = if file_fail == 0 { "PASS" } else { "FAIL" };
        println!(
            "{filename}: {status} — {file_pass}/{} passed",
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
    println!("=== Z80 SingleStepTests Summary ===");
    println!(
        "Files: {total_files}, Total: {}/{}, Pass: {total_pass}, Fail: {total_fail}",
        total_pass + total_fail,
        total_pass + total_fail
    );

    assert_eq!(total_fail, 0, "{total_fail} tests failed");
}
