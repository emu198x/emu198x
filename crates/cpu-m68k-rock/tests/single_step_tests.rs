//! Integration tests using SingleStepTests/m68000 test vectors.
//!
//! These tests verify that the cpu-m68k-rock Cpu68000 implementation produces
//! correct results against the m68000-dl single-step test suite.
//!
//! Key differences from cpu-m68k's harness:
//! - Reactive bus: `poll_cycle()` returns `BusStatus::Ready` immediately
//! - Crystal clock: `tick()` takes a clock counter, incremented by 4 per call
//! - DL cycle count = full hardware cycles (including opcode+IRC fetch time)

use cpu_m68k_rock::Cpu68000;
use cpu_m68k_rock::bus::{BusStatus, FunctionCode, M68kBus};
use std::fs;
use std::panic;
use std::path::Path;

/// Test bus: 16MB address space, instant DTACK, no contention.
struct TestBus {
    data: Vec<u8>,
}

impl TestBus {
    fn new() -> Self {
        Self {
            data: vec![0; 0x100_0000],
        }
    }

    fn load_ram(&mut self, ram: &[(u32, u8)]) {
        for &(addr, value) in ram {
            let addr24 = (addr & 0xFF_FFFF) as usize;
            self.data[addr24] = value;
        }
    }

    fn peek(&self, addr: u32) -> u8 {
        let addr24 = (addr & 0xFF_FFFF) as usize;
        self.data[addr24]
    }
}

impl M68kBus for TestBus {
    fn poll_cycle(
        &mut self,
        addr: u32,
        _fc: FunctionCode,
        is_read: bool,
        is_word: bool,
        data: Option<u16>,
    ) -> BusStatus {
        let addr24 = (addr & 0xFF_FFFF) as usize;
        if is_read {
            if is_word {
                let aligned = addr24 & 0xFF_FFFE;
                let hi = self.data[aligned];
                let lo = self.data[aligned + 1];
                BusStatus::Ready(u16::from(hi) << 8 | u16::from(lo))
            } else {
                BusStatus::Ready(u16::from(self.data[addr24]))
            }
        } else if let Some(value) = data {
            if is_word {
                let aligned = addr24 & 0xFF_FFFE;
                self.data[aligned] = (value >> 8) as u8;
                self.data[aligned + 1] = (value & 0xFF) as u8;
            } else {
                self.data[addr24] = (value & 0xFF) as u8;
            }
            BusStatus::Ready(0)
        } else {
            BusStatus::Ready(0)
        }
    }

    fn poll_ipl(&mut self) -> u8 {
        0 // No interrupts during tests
    }

    fn poll_interrupt_ack(&mut self, _level: u8) -> BusStatus {
        BusStatus::Ready(0)
    }

    fn reset(&mut self) {}
}

// --- Binary test format decoder ---
// (Copied from cpu-m68k/tests/single_step_tests.rs — format is CPU-independent)

#[derive(Debug, Clone)]
struct CpuState {
    d: [u32; 8],
    a: [u32; 7],
    usp: u32,
    ssp: u32,
    sr: u16,
    pc: u32,
    prefetch: [u32; 2],
    ram: Vec<(u32, u8)>,
}

#[derive(Debug)]
struct TestCase {
    name: String,
    initial: CpuState,
    final_state: CpuState,
    #[allow(dead_code)]
    cycles: u32,
}

fn decode_file(path: &Path) -> Result<Vec<TestCase>, String> {
    let content = fs::read(path).map_err(|e| format!("Failed to read file: {e}"))?;
    if content.len() < 8 {
        return Err("File too small".into());
    }

    let magic = u32::from_le_bytes([content[0], content[1], content[2], content[3]]);
    if magic != 0x1A3F_5D71 {
        return Err(format!("Invalid magic: 0x{magic:08X}"));
    }

    let num_tests =
        u32::from_le_bytes([content[4], content[5], content[6], content[7]]) as usize;
    let mut tests = Vec::with_capacity(num_tests);
    let mut ptr = 8;

    for _ in 0..num_tests {
        let (new_ptr, test) = decode_test(&content, ptr)?;
        ptr = new_ptr;
        tests.push(test);
    }

    Ok(tests)
}

fn decode_test(content: &[u8], mut ptr: usize) -> Result<(usize, TestCase), String> {
    if ptr + 8 > content.len() {
        return Err("Unexpected EOF at test header".into());
    }
    let magic = u32::from_le_bytes([
        content[ptr + 4],
        content[ptr + 5],
        content[ptr + 6],
        content[ptr + 7],
    ]);
    if magic != 0xABC1_2367 {
        return Err(format!("Invalid test magic: 0x{magic:08X}"));
    }
    ptr += 8;

    let (new_ptr, name) = read_name(content, ptr)?;
    ptr = new_ptr;
    let (new_ptr, initial) = read_state(content, ptr)?;
    ptr = new_ptr;
    let (new_ptr, final_state) = read_state(content, ptr)?;
    ptr = new_ptr;
    let (new_ptr, cycles) = read_transactions(content, ptr)?;
    ptr = new_ptr;

    Ok((
        ptr,
        TestCase {
            name,
            initial,
            final_state,
            cycles,
        },
    ))
}

fn read_name(content: &[u8], mut ptr: usize) -> Result<(usize, String), String> {
    if ptr + 8 > content.len() {
        return Err("Unexpected EOF at name header".into());
    }
    let magic = u32::from_le_bytes([
        content[ptr + 4],
        content[ptr + 5],
        content[ptr + 6],
        content[ptr + 7],
    ]);
    if magic != 0x89AB_CDEF {
        return Err(format!("Invalid name magic: 0x{magic:08X}"));
    }
    ptr += 8;

    if ptr + 4 > content.len() {
        return Err("Unexpected EOF at string length".into());
    }
    let strlen =
        u32::from_le_bytes([content[ptr], content[ptr + 1], content[ptr + 2], content[ptr + 3]])
            as usize;
    ptr += 4;

    if ptr + strlen > content.len() {
        return Err("Unexpected EOF at string data".into());
    }
    let name = String::from_utf8_lossy(&content[ptr..ptr + strlen]).to_string();
    ptr += strlen;

    Ok((ptr, name))
}

fn read_state(content: &[u8], mut ptr: usize) -> Result<(usize, CpuState), String> {
    if ptr + 8 > content.len() {
        return Err("Unexpected EOF at state header".into());
    }
    let magic = u32::from_le_bytes([
        content[ptr + 4],
        content[ptr + 5],
        content[ptr + 6],
        content[ptr + 7],
    ]);
    if magic != 0x0123_4567 {
        return Err(format!("Invalid state magic: 0x{magic:08X}"));
    }
    ptr += 8;

    if ptr + 19 * 4 > content.len() {
        return Err("Unexpected EOF at registers".into());
    }

    let read_u32 = |p: usize| -> u32 {
        u32::from_le_bytes([content[p], content[p + 1], content[p + 2], content[p + 3]])
    };

    let mut d = [0u32; 8];
    let mut a = [0u32; 7];
    for i in 0..8 {
        d[i] = read_u32(ptr + i * 4);
    }
    for i in 0..7 {
        a[i] = read_u32(ptr + 32 + i * 4);
    }

    let usp = read_u32(ptr + 60);
    let ssp = read_u32(ptr + 64);
    let sr = read_u32(ptr + 68) as u16;
    let pc = read_u32(ptr + 72);
    ptr += 76;

    if ptr + 8 > content.len() {
        return Err("Unexpected EOF at prefetch".into());
    }
    let prefetch = [read_u32(ptr), read_u32(ptr + 4)];
    ptr += 8;

    if ptr + 4 > content.len() {
        return Err("Unexpected EOF at RAM count".into());
    }
    let num_ram = read_u32(ptr) as usize;
    ptr += 4;

    if ptr + num_ram * 6 > content.len() {
        return Err("Unexpected EOF at RAM data".into());
    }

    let mut ram = Vec::with_capacity(num_ram * 2);
    for _ in 0..num_ram {
        let addr = read_u32(ptr);
        let data = u16::from_le_bytes([content[ptr + 4], content[ptr + 5]]);
        ptr += 6;
        ram.push((addr, (data >> 8) as u8));
        ram.push((addr | 1, (data & 0xFF) as u8));
    }

    Ok((
        ptr,
        CpuState {
            d,
            a,
            usp,
            ssp,
            sr,
            pc,
            prefetch,
            ram,
        },
    ))
}

fn read_transactions(content: &[u8], mut ptr: usize) -> Result<(usize, u32), String> {
    if ptr + 8 > content.len() {
        return Err("Unexpected EOF at transactions header".into());
    }
    let magic = u32::from_le_bytes([
        content[ptr + 4],
        content[ptr + 5],
        content[ptr + 6],
        content[ptr + 7],
    ]);
    if magic != 0x4567_89AB {
        return Err(format!("Invalid transactions magic: 0x{magic:08X}"));
    }
    ptr += 8;

    if ptr + 8 > content.len() {
        return Err("Unexpected EOF at cycle/transaction count".into());
    }
    let num_cycles =
        u32::from_le_bytes([content[ptr], content[ptr + 1], content[ptr + 2], content[ptr + 3]]);
    let num_transactions =
        u32::from_le_bytes([content[ptr + 4], content[ptr + 5], content[ptr + 6], content[ptr + 7]])
            as usize;
    ptr += 8;

    for _ in 0..num_transactions {
        if ptr >= content.len() {
            return Err("Unexpected EOF at transaction".into());
        }
        let tw = content[ptr];
        ptr += 5;
        if tw != 0 {
            ptr += 20;
        }
    }

    Ok((ptr, num_cycles))
}

// --- CPU setup and state comparison ---

fn setup_cpu(cpu: &mut Cpu68000, mem: &mut TestBus, state: &CpuState) {
    mem.load_ram(&state.ram);

    for i in 0..8 {
        cpu.regs.d[i] = state.d[i];
    }
    for i in 0..7 {
        cpu.regs.a[i] = state.a[i];
    }
    cpu.regs.usp = state.usp;
    cpu.regs.ssp = state.ssp;
    cpu.regs.sr = state.sr;
    cpu.regs.pc = state.pc;

    let opcode = state.prefetch[0] as u16;
    let irc = state.prefetch[1] as u16;
    cpu.setup_prefetch(opcode, irc);
}

fn compare_state(
    cpu: &Cpu68000,
    mem: &TestBus,
    expected: &CpuState,
    test_name: &str,
) -> Vec<String> {
    let mut errors = Vec::new();

    for i in 0..8 {
        if cpu.regs.d[i] != expected.d[i] {
            errors.push(format!(
                "{test_name}: D{i} mismatch: got 0x{:08X}, expected 0x{:08X}",
                cpu.regs.d[i], expected.d[i]
            ));
        }
    }
    for i in 0..7 {
        if cpu.regs.a[i] != expected.a[i] {
            errors.push(format!(
                "{test_name}: A{i} mismatch: got 0x{:08X}, expected 0x{:08X}",
                cpu.regs.a[i], expected.a[i]
            ));
        }
    }
    if cpu.regs.usp != expected.usp {
        errors.push(format!(
            "{test_name}: USP mismatch: got 0x{:08X}, expected 0x{:08X}",
            cpu.regs.usp, expected.usp
        ));
    }
    if cpu.regs.ssp != expected.ssp {
        errors.push(format!(
            "{test_name}: SSP mismatch: got 0x{:08X}, expected 0x{:08X}",
            cpu.regs.ssp, expected.ssp
        ));
    }
    if cpu.regs.sr != expected.sr {
        errors.push(format!(
            "{test_name}: SR mismatch: got 0x{:04X}, expected 0x{:04X}",
            cpu.regs.sr, expected.sr
        ));
    }
    if cpu.regs.pc != expected.pc {
        errors.push(format!(
            "{test_name}: PC mismatch: got 0x{:08X}, expected 0x{:08X}",
            cpu.regs.pc, expected.pc
        ));
    }

    for &(addr, expected_value) in &expected.ram {
        let actual = mem.peek(addr);
        if actual != expected_value {
            errors.push(format!(
                "{test_name}: RAM[0x{addr:06X}] mismatch: got 0x{actual:02X}, expected 0x{expected_value:02X}",
            ));
        }
    }

    errors
}

// --- Test runner ---

fn run_test(test: &TestCase) -> Result<(), Vec<String>> {
    let mut cpu = Cpu68000::new();
    let mut mem = TestBus::new();
    setup_cpu(&mut cpu, &mut mem, &test.initial);

    // Stop at the instruction boundary: when the tested instruction finishes
    // and the next instruction's Execute is queued but not yet run. This
    // avoids executing into random test memory.
    //
    // Use a generous upper bound to handle long instructions.
    let max_ticks = test.cycles.max(8) * 2;

    for i in 0..max_ticks {
        cpu.tick(&mut mem, u64::from(i) * 4);
        if cpu.is_halted() {
            break;
        }

        // Skip tick 0 — the initial Execute is still setting up the instruction.
        if i > 0
            && !cpu.in_followup
            && cpu.is_idle()
            && cpu.micro_ops.front().map_or(false, |op| {
                matches!(op, cpu_m68k_rock::microcode::MicroOp::Execute)
            })
        {
            break;
        }
    }

    let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Run a test with panic recovery for unimplemented instructions.
fn run_test_safe(test: &TestCase) -> Result<(), Vec<String>> {
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| run_test(test)));
    match result {
        Ok(r) => r,
        Err(_) => Err(vec![format!("{}: PANIC (unimplemented instruction)", test.name)]),
    }
}

fn run_test_file_inner(path: &Path, safe: bool) -> (usize, usize, Vec<String>) {
    let tests = match decode_file(path) {
        Ok(t) => t,
        Err(e) => return (0, 0, vec![format!("Failed to decode {}: {e}", path.display())]),
    };

    let mut passed = 0;
    let mut failed = 0;
    let mut all_errors = Vec::new();

    for test in &tests {
        let result = if safe { run_test_safe(test) } else { run_test(test) };
        match result {
            Ok(()) => passed += 1,
            Err(errors) => {
                failed += 1;
                if all_errors.len() < 20 {
                    all_errors.extend(errors.into_iter().take(3));
                }
            }
        }
    }

    (passed, failed, all_errors)
}

fn run_named_test(name: &str) {
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(format!("test-data/m68000-dl/v1/{name}.json.bin"));

    if !test_file.exists() {
        eprintln!("Test file not found: {}", test_file.display());
        return;
    }

    // Use safe mode (catch_unwind) because the next instruction decoded
    // from random test memory may hit unimplemented code paths.
    let (passed, failed, errors) = run_test_file_inner(&test_file, true);
    println!("{name} tests: {passed} passed, {failed} failed");
    if !errors.is_empty() {
        println!("First errors:");
        for err in errors.iter().take(10) {
            println!("  {err}");
        }
    }
    assert_eq!(failed, 0, "{name}: {failed} tests failed");
}

// --- Individual instruction tests ---

#[test]
fn test_moveq() {
    run_named_test("MOVE.q");
}

#[test]
fn test_nop() {
    run_named_test("NOP");
}

/// Run a single test file specified by ROCK_TEST_FILE env var.
/// Usage: ROCK_TEST_FILE=TST.b cargo test -p cpu-m68k-rock --test single_step_tests run_single_file -- --ignored --nocapture
#[test]
#[ignore]
fn run_single_file() {
    let name = match std::env::var("ROCK_TEST_FILE") {
        Ok(n) => n,
        Err(_) => {
            eprintln!("Set ROCK_TEST_FILE=<name> (e.g. TST.b)");
            return;
        }
    };
    run_named_test(&name);
}


// --- Full suite (run manually) ---

#[test]
#[ignore]
fn run_all_single_step_tests() {
    let test_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-data/m68000-dl/v1");

    if !test_dir.exists() {
        eprintln!("Test directory not found: {}", test_dir.display());
        return;
    }

    let mut total_passed = 0;
    let mut total_failed = 0;

    let mut entries: Vec<_> = fs::read_dir(&test_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "bin")
                .unwrap_or(false)
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in &entries {
        let path = entry.path();
        let (passed, failed, errors) = run_test_file_inner(&path, true);
        let stem = path.file_stem().unwrap().to_string_lossy();
        if failed > 0 {
            println!("{stem}: {passed} passed, {failed} failed");
            for err in errors.iter().take(3) {
                println!("  {err}");
            }
        } else {
            println!("{stem}: {passed} passed");
        }

        total_passed += passed;
        total_failed += failed;
    }

    println!("\n=== Total: {total_passed} passed, {total_failed} failed ===");
}
