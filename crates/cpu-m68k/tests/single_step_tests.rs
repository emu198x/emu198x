//! Integration tests using SingleStepTests/m68000 test vectors.
//!
//! These tests verify that the cpu-m68k Cpu68000 implementation produces correct
//! results against the 317,500 single-step tests from the m68000-dl test suite.
//!
//! The tests use a TestBus that implements M68kBus directly (word-level access,
//! zero wait states).
//!
//! Key difference from emu-m68k: no ext_words array construction. Just
//! setup_prefetch(opcode, irc) — extension words come from IRC at decode time.

use cpu_m68k::Cpu68000;
use cpu_m68k::bus::{BusResult, FunctionCode, M68kBus};
use std::fs;
use std::path::Path;

/// Extended memory bus for tests - full 16MB address space (24-bit).
struct TestBus {
    data: Vec<u8>,
}

impl TestBus {
    fn new() -> Self {
        Self {
            // Full 16MB address space for 68000 (24-bit addresses)
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
    fn read_word(&mut self, addr: u32, _fc: FunctionCode) -> BusResult {
        let addr24 = (addr & 0xFF_FFFE) as usize;
        let hi = self.data[addr24];
        let lo = self.data[addr24 + 1];
        BusResult::new(u16::from(hi) << 8 | u16::from(lo))
    }

    fn write_word(&mut self, addr: u32, value: u16, _fc: FunctionCode) -> BusResult {
        let addr24 = (addr & 0xFF_FFFE) as usize;
        self.data[addr24] = (value >> 8) as u8;
        self.data[addr24 + 1] = (value & 0xFF) as u8;
        BusResult::write_ok()
    }

    fn read_byte(&mut self, addr: u32, _fc: FunctionCode) -> BusResult {
        let addr24 = (addr & 0xFF_FFFF) as usize;
        BusResult::new(u16::from(self.data[addr24]))
    }

    fn write_byte(&mut self, addr: u32, value: u8, _fc: FunctionCode) -> BusResult {
        let addr24 = (addr & 0xFF_FFFF) as usize;
        self.data[addr24] = value;
        BusResult::write_ok()
    }
}

/// Decoded CPU state from test file.
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

/// A single test case.
#[derive(Debug)]
struct TestCase {
    name: String,
    initial: CpuState,
    final_state: CpuState,
    #[allow(dead_code)]
    cycles: u32,
}

/// Decode a test file in the SingleStepTests binary format.
fn decode_file(path: &Path) -> Result<Vec<TestCase>, String> {
    let content = fs::read(path).map_err(|e| format!("Failed to read file: {e}"))?;

    if content.len() < 8 {
        return Err("File too small".into());
    }

    let magic = u32::from_le_bytes([content[0], content[1], content[2], content[3]]);
    if magic != 0x1A3F_5D71 {
        return Err(format!("Invalid magic: 0x{magic:08X}"));
    }

    let num_tests = u32::from_le_bytes([content[4], content[5], content[6], content[7]]) as usize;

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

    let strlen = u32::from_le_bytes([content[ptr], content[ptr + 1], content[ptr + 2], content[ptr + 3]])
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

    let num_cycles = u32::from_le_bytes([content[ptr], content[ptr + 1], content[ptr + 2], content[ptr + 3]]);
    let num_transactions =
        u32::from_le_bytes([content[ptr + 4], content[ptr + 5], content[ptr + 6], content[ptr + 7]]) as usize;
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

/// Apply initial state to CPU and memory.
///
/// Key difference from emu-m68k: we use setup_prefetch(opcode, irc)
/// instead of building an ext_words array. Extension words come from
/// IRC at decode time, not from a pre-loaded array.
fn setup_cpu(cpu: &mut Cpu68000, mem: &mut TestBus, state: &CpuState) {
    // Load RAM first (includes instruction bytes)
    mem.load_ram(&state.ram);

    // Set registers
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

    // Set up prefetch — just IR and IRC, no ext_words array
    let opcode = state.prefetch[0] as u16;
    let irc = state.prefetch[1] as u16;
    cpu.setup_prefetch(opcode, irc);
}

/// Compare CPU state with expected final state.
fn compare_state(cpu: &Cpu68000, mem: &TestBus, expected: &CpuState, test_name: &str) -> Vec<String> {
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

/// Run a single test case.
fn run_test(test: &TestCase) -> Result<(), Vec<String>> {
    let mut cpu = Cpu68000::new();
    let mut mem = TestBus::new();

    setup_cpu(&mut cpu, &mut mem, &test.initial);

    let cycles_to_run = if test.cycles > 0 { test.cycles } else { 8 };

    for _ in 0..cycles_to_run {
        cpu.tick(&mut mem);
    }

    let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Run all tests from a file and return (passed, failed, errors).
fn run_test_file(path: &Path) -> (usize, usize, Vec<String>) {
    let tests = match decode_file(path) {
        Ok(t) => t,
        Err(e) => return (0, 0, vec![format!("Failed to decode {}: {e}", path.display())]),
    };

    let mut passed = 0;
    let mut failed = 0;
    let mut all_errors = Vec::new();

    for test in &tests {
        match run_test(test) {
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

/// Smoke test: run MOVEA.w tests (should all fail with illegal instruction at Phase 0).
#[test]
fn test_movea_w() {
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-data/m68000-dl/v1/MOVEA.w.json.bin");

    if !test_file.exists() {
        eprintln!("Test file not found: {}", test_file.display());
        return;
    }

    let (passed, failed, errors) = run_test_file(&test_file);
    println!("MOVEA.w tests: {passed} passed, {failed} failed");
    if !errors.is_empty() {
        println!("First errors:");
        for err in errors.iter().take(10) {
            println!("  {err}");
        }
    }
}

/// Run all available test files (called manually, not in CI).
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
        .filter(|e| e.path().extension().map(|ext| ext == "bin").unwrap_or(false))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in &entries {
        let path = entry.path();
        let (passed, failed, errors) = run_test_file(&path);
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
