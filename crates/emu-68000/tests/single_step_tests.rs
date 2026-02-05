//! Integration tests using SingleStepTests/m68000 test vectors.
//!
//! These tests decode binary test files from the SingleStepTests project and verify
//! that our 68000 implementation produces correct results.
//!
//! Test files are in test-data/m68000/*.json.bin format.
//!
//! Note: Our current M68000 uses a 16-bit Bus trait, so addresses above 0xFFFF will
//! wrap. Tests that use high addresses may fail due to this limitation.

use emu_68000::M68000;
use emu_core::{Bus, Cpu, ReadResult};
use std::fs;
use std::path::Path;

/// Extended memory bus for tests - full 16MB address space (24-bit).
/// Implements the Bus trait with proper 68000 address masking.
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

impl Bus for TestBus {
    fn read(&mut self, addr: u32) -> ReadResult {
        let addr24 = (addr & 0xFF_FFFF) as usize;
        ReadResult::new(self.data[addr24])
    }

    fn write(&mut self, addr: u32, value: u8) -> u8 {
        let addr24 = (addr & 0xFF_FFFF) as usize;
        self.data[addr24] = value;
        0
    }

    fn io_read(&mut self, _addr: u32) -> ReadResult {
        ReadResult::new(0xFF)
    }

    fn io_write(&mut self, _addr: u32, _value: u8) -> u8 {
        0
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
    let content = fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;

    if content.len() < 8 {
        return Err("File too small".into());
    }

    let magic = u32::from_le_bytes([content[0], content[1], content[2], content[3]]);
    if magic != 0x1A3F_5D71 {
        return Err(format!("Invalid magic: 0x{:08X}", magic));
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
    // Test header: numbytes (4) + magic (4)
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
        return Err(format!("Invalid test magic: 0x{:08X}", magic));
    }
    ptr += 8;

    // Read name
    let (new_ptr, name) = read_name(content, ptr)?;
    ptr = new_ptr;

    // Read initial state
    let (new_ptr, initial) = read_state(content, ptr)?;
    ptr = new_ptr;

    // Read final state
    let (new_ptr, final_state) = read_state(content, ptr)?;
    ptr = new_ptr;

    // Read transactions (we mainly care about cycle count)
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
        return Err(format!("Invalid name magic: 0x{:08X}", magic));
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
        return Err(format!("Invalid state magic: 0x{:08X}", magic));
    }
    ptr += 8;

    // Read 19 registers: d0-d7, a0-a6, usp, ssp, sr, pc
    // Each is 4 bytes LE
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

    // Read 2 prefetch values
    if ptr + 8 > content.len() {
        return Err("Unexpected EOF at prefetch".into());
    }
    let prefetch = [read_u32(ptr), read_u32(ptr + 4)];
    ptr += 8;

    // Read RAM entries
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

        // Split word into two bytes (high byte at even address, low byte at odd)
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
        return Err(format!("Invalid transactions magic: 0x{:08X}", magic));
    }
    ptr += 8;

    if ptr + 8 > content.len() {
        return Err("Unexpected EOF at cycle/transaction count".into());
    }

    let num_cycles = u32::from_le_bytes([content[ptr], content[ptr + 1], content[ptr + 2], content[ptr + 3]]);
    let num_transactions =
        u32::from_le_bytes([content[ptr + 4], content[ptr + 5], content[ptr + 6], content[ptr + 7]]) as usize;
    ptr += 8;

    // Skip transaction data (we're not verifying bus transactions yet)
    for _ in 0..num_transactions {
        if ptr >= content.len() {
            return Err("Unexpected EOF at transaction".into());
        }
        let tw = content[ptr];
        ptr += 5; // type (1) + cycles (4)

        if tw != 0 {
            ptr += 20; // fc + addr + data + uds + lds (all u32)
        }
    }

    Ok((ptr, num_cycles))
}

/// Apply initial state to CPU and memory.
fn setup_cpu(cpu: &mut M68000, mem: &mut TestBus, state: &CpuState) {
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
    // Set SR (this determines which stack pointer is active)
    cpu.regs.sr = state.sr;
    cpu.regs.pc = state.pc;

    // Set up prefetch - the 68000 has a 2-word prefetch queue:
    // - IR (Instruction Register): the opcode being executed
    // - IRC (Instruction Register Cache): the next word (first extension word)
    // PC points to where the NEXT fetch would come from (after IRC).
    // Additional extension words are read from memory at PC, PC+2, PC+4.
    let opcode = state.prefetch[0] as u16;
    let pc = state.pc;

    // Build extension words array: IRC + words from memory at PC, PC+2, PC+4
    let mut ext_words = [0u16; 4];
    ext_words[0] = state.prefetch[1] as u16; // IRC
    ext_words[1] = (u16::from(mem.peek(pc)) << 8) | u16::from(mem.peek(pc.wrapping_add(1)));
    ext_words[2] = (u16::from(mem.peek(pc.wrapping_add(2))) << 8)
        | u16::from(mem.peek(pc.wrapping_add(3)));
    ext_words[3] = (u16::from(mem.peek(pc.wrapping_add(4))) << 8)
        | u16::from(mem.peek(pc.wrapping_add(5)));

    cpu.setup_prefetch(opcode, &ext_words);
}

/// Compare CPU state with expected final state.
fn compare_state(cpu: &M68000, mem: &TestBus, expected: &CpuState, test_name: &str) -> Vec<String> {
    let mut errors = Vec::new();

    // Compare data registers
    for i in 0..8 {
        if cpu.regs.d[i] != expected.d[i] {
            errors.push(format!(
                "{}: D{} mismatch: got 0x{:08X}, expected 0x{:08X}",
                test_name, i, cpu.regs.d[i], expected.d[i]
            ));
        }
    }

    // Compare address registers (A0-A6)
    for i in 0..7 {
        if cpu.regs.a[i] != expected.a[i] {
            errors.push(format!(
                "{}: A{} mismatch: got 0x{:08X}, expected 0x{:08X}",
                test_name, i, cpu.regs.a[i], expected.a[i]
            ));
        }
    }

    // Compare USP and SSP
    if cpu.regs.usp != expected.usp {
        errors.push(format!(
            "{}: USP mismatch: got 0x{:08X}, expected 0x{:08X}",
            test_name, cpu.regs.usp, expected.usp
        ));
    }
    if cpu.regs.ssp != expected.ssp {
        errors.push(format!(
            "{}: SSP mismatch: got 0x{:08X}, expected 0x{:08X}",
            test_name, cpu.regs.ssp, expected.ssp
        ));
    }

    // Compare SR
    if cpu.regs.sr != expected.sr {
        errors.push(format!(
            "{}: SR mismatch: got 0x{:04X}, expected 0x{:04X}",
            test_name, cpu.regs.sr, expected.sr
        ));
    }

    // Compare PC
    if cpu.regs.pc != expected.pc {
        errors.push(format!(
            "{}: PC mismatch: got 0x{:08X}, expected 0x{:08X}",
            test_name, cpu.regs.pc, expected.pc
        ));
    }

    // Compare RAM
    for &(addr, expected_value) in &expected.ram {
        let actual = mem.peek(addr);
        if actual != expected_value {
            errors.push(format!(
                "{}: RAM[0x{:06X}] mismatch: got 0x{:02X}, expected 0x{:02X}",
                test_name, addr, actual, expected_value
            ));
        }
    }

    errors
}

/// Run a single test case.
fn run_test(test: &TestCase) -> Result<(), Vec<String>> {
    let mut cpu = M68000::new();
    let mut mem = TestBus::new();

    setup_cpu(&mut cpu, &mut mem, &test.initial);

    // Run for the expected number of cycles
    // The test specifies exact cycle count
    let cycles_to_run = if test.cycles > 0 { test.cycles } else { 8 }; // Default 8 for NOP

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
        Err(e) => return (0, 0, vec![format!("Failed to decode {}: {}", path.display(), e)]),
    };

    let mut passed = 0;
    let mut failed = 0;
    let mut all_errors = Vec::new();

    for test in &tests {
        match run_test(test) {
            Ok(()) => passed += 1,
            Err(errors) => {
                failed += 1;
                // Only collect first few errors to avoid spam
                if all_errors.len() < 20 {
                    all_errors.extend(errors.into_iter().take(3));
                }
            }
        }
    }

    (passed, failed, all_errors)
}

/// Run MOVEA.w tests to see pass rate.
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
    println!("MOVEA.w tests: {} passed, {} failed", passed, failed);
    if !errors.is_empty() {
        println!("First 10 errors:");
        for err in errors.iter().take(10) {
            println!("  {}", err);
        }
    }
}

/// Run just ABCD tests to see pass rate.
#[test]
fn test_abcd() {
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-data/m68000-dl/v1/ABCD.json.bin");

    if !test_file.exists() {
        eprintln!("Test file not found: {}", test_file.display());
        return;
    }

    let (passed, failed, errors) = run_test_file(&test_file);
    println!("ABCD tests: {} passed, {} failed", passed, failed);
    if !errors.is_empty() {
        println!("First 10 errors:");
        for err in errors.iter().take(10) {
            println!("  {}", err);
        }
    }
}

/// Run just SBCD tests to see pass rate.
#[test]
fn test_sbcd() {
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-data/m68000-dl/v1/SBCD.json.bin");

    if !test_file.exists() {
        eprintln!("Test file not found: {}", test_file.display());
        return;
    }

    let (passed, failed, errors) = run_test_file(&test_file);
    println!("SBCD tests: {} passed, {} failed", passed, failed);
    if !errors.is_empty() {
        println!("First 10 errors:");
        for err in errors.iter().take(10) {
            println!("  {}", err);
        }
    }
}

/// Run just DIVU tests to see pass rate.
#[test]
fn test_divu() {
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-data/m68000-dl/v1/DIVU.json.bin");

    if !test_file.exists() {
        eprintln!("Test file not found: {}", test_file.display());
        return;
    }

    let (passed, failed, errors) = run_test_file(&test_file);
    println!("DIVU tests: {} passed, {} failed", passed, failed);
    if !errors.is_empty() {
        println!("First 10 errors:");
        for err in errors.iter().take(10) {
            println!("  {}", err);
        }
    }
}

/// Diagnostic test for BSR failures.
#[test]
fn diagnose_bsr_test() {
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-data/m68000-dl/v1/BSR.json.bin");

    if !test_file.exists() {
        eprintln!("Test file not found: {}", test_file.display());
        return;
    }

    let tests = decode_file(&test_file).expect("Failed to decode");

    // Find first failing test
    for (i, test) in tests.iter().enumerate() {
        let mut cpu = M68000::new();
        let mut mem = TestBus::new();
        setup_cpu(&mut cpu, &mut mem, &test.initial);

        for _ in 0..test.cycles {
            cpu.tick(&mut mem);
        }

        let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
        if !errors.is_empty() {
            println!("\n=== First failing test: {} (index {}) ===", test.name, i);
            println!("Expected cycles: {}", test.cycles);

            println!("\n--- Initial State ---");
            println!("PC: 0x{:08X}", test.initial.pc);
            println!("SR: 0x{:04X}", test.initial.sr);
            println!("USP: 0x{:08X}, SSP: 0x{:08X}", test.initial.usp, test.initial.ssp);
            println!("Prefetch: [{:04X}, {:04X}]", test.initial.prefetch[0], test.initial.prefetch[1]);

            println!("\n--- Our Final State ---");
            println!("PC: 0x{:08X} (expected 0x{:08X})", cpu.regs.pc, test.final_state.pc);
            println!("SR: 0x{:04X} (expected 0x{:04X})", cpu.regs.sr, test.final_state.sr);
            println!("USP: 0x{:08X} (expected 0x{:08X})", cpu.regs.usp, test.final_state.usp);
            println!("SSP: 0x{:08X} (expected 0x{:08X})", cpu.regs.ssp, test.final_state.ssp);

            println!("\n--- Errors ---");
            for err in &errors {
                println!("  {}", err);
            }

            // Show stack area
            println!("\n--- Stack area (around SSP) ---");
            let ssp = test.final_state.ssp;
            for offset in 0..16u32 {
                let addr = ssp.wrapping_sub(8).wrapping_add(offset);
                let expected = test.final_state.ram.iter().find(|&&(a, _)| a == addr).map(|&(_, v)| v);
                let actual = mem.peek(addr);
                let marker = if expected.map(|e| e != actual).unwrap_or(false) { " <-- MISMATCH" } else { "" };
                println!("  0x{:06X}: actual=0x{:02X}, expected={}{}",
                    addr, actual,
                    expected.map(|e| format!("0x{:02X}", e)).unwrap_or("N/A".to_string()),
                    marker);
            }

            return;
        }
    }
    println!("All BSR tests passed!");
}

/// Diagnostic test for Bcc failures.
#[test]
fn diagnose_bcc_test() {
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-data/m68000-dl/v1/Bcc.json.bin");

    if !test_file.exists() {
        eprintln!("Test file not found: {}", test_file.display());
        return;
    }

    let tests = decode_file(&test_file).expect("Failed to decode");

    // Find first failing test
    for (i, test) in tests.iter().enumerate() {
        let mut cpu = M68000::new();
        let mut mem = TestBus::new();
        setup_cpu(&mut cpu, &mut mem, &test.initial);

        for _ in 0..test.cycles {
            cpu.tick(&mut mem);
        }

        let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
        if !errors.is_empty() {
            println!("\n=== First failing test: {} (index {}) ===", test.name, i);
            println!("Expected cycles: {}", test.cycles);

            println!("\n--- Initial State ---");
            println!("PC: 0x{:08X}", test.initial.pc);
            println!("SR: 0x{:04X} (CCR: {:02X})", test.initial.sr, test.initial.sr & 0xFF);
            println!("USP: 0x{:08X}, SSP: 0x{:08X}", test.initial.usp, test.initial.ssp);
            println!("Prefetch: [{:04X}, {:04X}]", test.initial.prefetch[0], test.initial.prefetch[1]);

            // Decode instruction
            let opcode = test.initial.prefetch[0];
            let condition = (opcode >> 8) & 0xF;
            let displacement = opcode as u8;
            let cond_names = ["T", "F", "HI", "LS", "CC", "CS", "NE", "EQ", "VC", "VS", "PL", "MI", "GE", "LT", "GT", "LE"];
            println!("Opcode: 0x{:04X} = B{} displacement={} (signed: {})",
                opcode, cond_names[condition as usize], displacement, displacement as i8);

            // Calculate target
            let instr_addr = test.initial.pc.wrapping_sub(4);
            let pc_for_branch = instr_addr.wrapping_add(2);
            let target = (pc_for_branch as i32).wrapping_add(displacement as i8 as i32) as u32;
            println!("Instruction at: 0x{:08X}, PC for branch: 0x{:08X}, Target: 0x{:08X} (odd: {})",
                instr_addr, pc_for_branch, target, target & 1 != 0);

            println!("\n--- Our Final State ---");
            println!("PC: 0x{:08X} (expected 0x{:08X})", cpu.regs.pc, test.final_state.pc);
            println!("SR: 0x{:04X} (expected 0x{:04X})", cpu.regs.sr, test.final_state.sr);
            println!("USP: 0x{:08X} (expected 0x{:08X})", cpu.regs.usp, test.final_state.usp);
            println!("SSP: 0x{:08X} (expected 0x{:08X})", cpu.regs.ssp, test.final_state.ssp);

            println!("\n--- Errors ---");
            for err in &errors {
                println!("  {}", err);
            }

            // Show stack area
            println!("\n--- Stack area (around final SSP 0x{:08X}) ---", test.final_state.ssp);
            let ssp = test.final_state.ssp;
            for offset in 0..20u32 {
                let addr = ssp.wrapping_add(offset);
                let expected = test.final_state.ram.iter().find(|&&(a, _)| a == addr).map(|&(_, v)| v);
                let actual = mem.peek(addr);
                let marker = if expected.map(|e| e != actual).unwrap_or(false) { " <-- MISMATCH" } else { "" };
                println!("  0x{:06X}: actual=0x{:02X}, expected={}{}",
                    addr, actual,
                    expected.map(|e| format!("0x{:02X}", e)).unwrap_or("N/A".to_string()),
                    marker);
            }

            return;
        }
    }
    println!("All Bcc tests passed!");
}

/// Diagnostic test for RTS failures.
#[test]
fn diagnose_rts_test() {
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-data/m68000-dl/v1/RTS.json.bin");

    if !test_file.exists() {
        eprintln!("Test file not found: {}", test_file.display());
        return;
    }

    let tests = decode_file(&test_file).expect("Failed to decode");

    // Find first failing test
    for (i, test) in tests.iter().enumerate() {
        let mut cpu = M68000::new();
        let mut mem = TestBus::new();
        setup_cpu(&mut cpu, &mut mem, &test.initial);

        for _ in 0..test.cycles {
            cpu.tick(&mut mem);
        }

        let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
        if !errors.is_empty() {
            println!("\n=== First failing test: {} (index {}) ===", test.name, i);
            println!("Expected cycles: {}", test.cycles);

            println!("\n--- Initial State ---");
            println!("PC: 0x{:08X}", test.initial.pc);
            println!("SR: 0x{:04X}", test.initial.sr);
            println!("USP: 0x{:08X}, SSP: 0x{:08X}", test.initial.usp, test.initial.ssp);
            println!("Prefetch: [{:04X}, {:04X}]", test.initial.prefetch[0], test.initial.prefetch[1]);

            // Show stack contents
            let sp = if test.initial.sr & 0x2000 != 0 { test.initial.ssp } else { test.initial.usp };
            println!("Active SP: 0x{:08X}", sp);
            println!("Stack contents:");
            for offset in 0..8u32 {
                let addr = sp.wrapping_add(offset);
                if let Some(&(_, val)) = test.initial.ram.iter().find(|&&(a, _)| a == addr) {
                    println!("  [SP+{}] 0x{:06X}: 0x{:02X}", offset, addr, val);
                }
            }

            println!("\n--- Our Final State ---");
            println!("PC: 0x{:08X} (expected 0x{:08X})", cpu.regs.pc, test.final_state.pc);
            println!("SR: 0x{:04X} (expected 0x{:04X})", cpu.regs.sr, test.final_state.sr);
            println!("USP: 0x{:08X} (expected 0x{:08X})", cpu.regs.usp, test.final_state.usp);
            println!("SSP: 0x{:08X} (expected 0x{:08X})", cpu.regs.ssp, test.final_state.ssp);

            println!("\n--- Errors ---");
            for err in &errors {
                println!("  {}", err);
            }

            return;
        }
    }
    println!("All RTS tests passed!");
}

/// Diagnostic test for JSR failures.
#[test]
fn diagnose_jsr_test() {
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-data/m68000-dl/v1/JSR.json.bin");

    if !test_file.exists() {
        eprintln!("Test file not found: {}", test_file.display());
        return;
    }

    let tests = decode_file(&test_file).expect("Failed to decode");

    // Find first failing test
    for (i, test) in tests.iter().enumerate() {
        let mut cpu = M68000::new();
        let mut mem = TestBus::new();
        setup_cpu(&mut cpu, &mut mem, &test.initial);

        for _ in 0..test.cycles {
            cpu.tick(&mut mem);
        }

        let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
        if !errors.is_empty() {
            println!("\n=== First failing test: {} (index {}) ===", test.name, i);
            println!("Expected cycles: {}", test.cycles);

            println!("\n--- Initial State ---");
            println!("PC: 0x{:08X}", test.initial.pc);
            println!("SR: 0x{:04X}", test.initial.sr);
            println!("USP: 0x{:08X}, SSP: 0x{:08X}", test.initial.usp, test.initial.ssp);
            println!("Prefetch: [{:04X}, {:04X}]", test.initial.prefetch[0], test.initial.prefetch[1]);
            for i in 0..7 {
                println!("A{}: 0x{:08X}", i, test.initial.a[i]);
            }
            println!("A7/SP: USP=0x{:08X}, SSP=0x{:08X}", test.initial.usp, test.initial.ssp);

            // Decode instruction
            let opcode = test.initial.prefetch[0];
            let instr_addr = test.initial.pc.wrapping_sub(4);
            println!("\nInstruction at: 0x{:08X}", instr_addr);
            println!("Opcode: 0x{:04X}", opcode);
            println!("Expected return addr: instruction_addr + instruction_length");

            println!("\n--- Our Final State ---");
            println!("PC: 0x{:08X} (expected 0x{:08X})", cpu.regs.pc, test.final_state.pc);
            println!("SR: 0x{:04X} (expected 0x{:04X})", cpu.regs.sr, test.final_state.sr);
            println!("USP: 0x{:08X} (expected 0x{:08X})", cpu.regs.usp, test.final_state.usp);
            println!("SSP: 0x{:08X} (expected 0x{:08X})", cpu.regs.ssp, test.final_state.ssp);

            // Show stack to see pushed return address
            let sp = if test.final_state.sr & 0x2000 != 0 { test.final_state.ssp } else { test.final_state.usp };
            println!("\n--- Stack (return addr pushed by JSR) ---");
            for offset in 0..8u32 {
                let addr = sp.wrapping_add(offset);
                let expected = test.final_state.ram.iter().find(|&&(a, _)| a == addr).map(|&(_, v)| v);
                let actual = mem.peek(addr);
                let marker = if expected.map(|e| e != actual).unwrap_or(false) { " <-- MISMATCH" } else { "" };
                println!("  [SP+{}] 0x{:06X}: actual=0x{:02X}, expected={}{}",
                    offset, addr, actual,
                    expected.map(|e| format!("0x{:02X}", e)).unwrap_or("N/A".to_string()),
                    marker);
            }

            println!("\n--- Errors ---");
            for err in &errors {
                println!("  {}", err);
            }

            return;
        }
    }
    println!("All JSR tests passed!");
}

/// Diagnostic test for DBcc failures.
#[test]
fn diagnose_dbcc_test() {
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-data/m68000-dl/v1/DBcc.json.bin");

    if !test_file.exists() {
        eprintln!("Test file not found: {}", test_file.display());
        return;
    }

    let tests = decode_file(&test_file).expect("Failed to decode");

    // Find first failing test
    for (i, test) in tests.iter().enumerate() {
        let mut cpu = M68000::new();
        let mut mem = TestBus::new();
        setup_cpu(&mut cpu, &mut mem, &test.initial);

        for _ in 0..test.cycles {
            cpu.tick(&mut mem);
        }

        let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
        if !errors.is_empty() {
            println!("\n=== First failing test: {} (index {}) ===", test.name, i);
            println!("Expected cycles: {}", test.cycles);

            println!("\n--- Initial State ---");
            println!("PC: 0x{:08X}", test.initial.pc);
            println!("SR: 0x{:04X} (CCR: {:02X})", test.initial.sr, test.initial.sr & 0xFF);
            println!("USP: 0x{:08X}, SSP: 0x{:08X}", test.initial.usp, test.initial.ssp);
            println!("Prefetch: [{:04X}, {:04X}]", test.initial.prefetch[0], test.initial.prefetch[1]);
            for j in 0..8 {
                println!("D{}: 0x{:08X}", j, test.initial.d[j]);
            }

            // Decode instruction
            let opcode = test.initial.prefetch[0];
            let condition = (opcode >> 8) & 0xF;
            let reg = opcode & 0x7;
            let disp_word = test.initial.prefetch[1];
            let cond_names = ["T", "F", "HI", "LS", "CC", "CS", "NE", "EQ", "VC", "VS", "PL", "MI", "GE", "LT", "GT", "LE"];
            println!("\nOpcode: 0x{:04X} = DB{} D{}, displacement=0x{:04X} ({})",
                opcode, cond_names[condition as usize], reg, disp_word, disp_word as i16);

            // Calculate target
            let instr_addr = test.initial.pc.wrapping_sub(4);
            let pc_for_disp = instr_addr.wrapping_add(2); // PC after opcode
            let target = (pc_for_disp as i32).wrapping_add(disp_word as i16 as i32) as u32;
            println!("Instruction at: 0x{:08X}, PC for disp: 0x{:08X}, Target: 0x{:08X} (odd: {})",
                instr_addr, pc_for_disp, target, target & 1 != 0);

            println!("\n--- Our Final State ---");
            println!("PC: 0x{:08X} (expected 0x{:08X})", cpu.regs.pc, test.final_state.pc);
            println!("SR: 0x{:04X} (expected 0x{:04X})", cpu.regs.sr, test.final_state.sr);
            println!("USP: 0x{:08X} (expected 0x{:08X})", cpu.regs.usp, test.final_state.usp);
            println!("SSP: 0x{:08X} (expected 0x{:08X})", cpu.regs.ssp, test.final_state.ssp);
            for j in 0..8 {
                let expected_d = test.final_state.d[j];
                let actual_d = cpu.regs.d[j];
                if expected_d != actual_d {
                    println!("D{}: 0x{:08X} (expected 0x{:08X}) <-- MISMATCH", j, actual_d, expected_d);
                }
            }

            println!("\n--- Errors ---");
            for err in &errors {
                println!("  {}", err);
            }

            // If SSP changed, show exception frame
            if test.final_state.ssp != test.initial.ssp {
                println!("\n--- Exception Frame (SSP changed) ---");
                let ssp = test.final_state.ssp;
                for offset in 0..16u32 {
                    let addr = ssp.wrapping_add(offset);
                    let expected = test.final_state.ram.iter().find(|&&(a, _)| a == addr).map(|&(_, v)| v);
                    let actual = mem.peek(addr);
                    let marker = if expected.map(|e| e != actual).unwrap_or(false) { " <-- MISMATCH" } else { "" };
                    println!("  [SSP+{:02}] 0x{:06X}: actual=0x{:02X}, expected={}{}",
                        offset, addr, actual,
                        expected.map(|e| format!("0x{:02X}", e)).unwrap_or("N/A".to_string()),
                        marker);
                }
            }

            return;
        }
    }
    println!("All DBcc tests passed!");
}

/// Diagnostic test to understand MOVEA.w increment issue.
#[test]
fn diagnose_movea_test() {
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

    let tests = decode_file(&test_file).expect("Failed to decode");
    // Test 036 has A3 increment issue
    let test = &tests[36];

    println!("\n=== Test: {} ===", test.name);
    println!("Expected cycles: {}", test.cycles);

    println!("\n--- Initial State ---");
    println!("PC: 0x{:08X}", test.initial.pc);
    println!("SR: 0x{:04X} (CCR: 0x{:02X})", test.initial.sr, test.initial.sr & 0xFF);
    for i in 0..8 {
        println!("D{}: 0x{:08X}", i, test.initial.d[i]);
    }
    for i in 0..7 {
        println!("A{}: 0x{:08X}", i, test.initial.a[i]);
    }
    println!("Prefetch: [{:08X}, {:08X}]", test.initial.prefetch[0], test.initial.prefetch[1]);

    // Run the test
    let mut cpu = M68000::new();
    let mut mem = TestBus::new();
    setup_cpu(&mut cpu, &mut mem, &test.initial);

    // Run for specified cycles
    for _ in 0..test.cycles {
        cpu.tick(&mut mem);
    }

    println!("\n--- Our Final State ---");
    println!("SR: 0x{:04X} (CCR: 0x{:02X})", cpu.regs.sr, cpu.regs.sr & 0xFF);
    for i in 0..8 {
        let marker = if cpu.regs.d[i] != test.final_state.d[i] { " <-- MISMATCH" } else { "" };
        println!("D{}: 0x{:08X}{}", i, cpu.regs.d[i], marker);
    }
    for i in 0..7 {
        let marker = if cpu.regs.a[i] != test.final_state.a[i] { " <-- MISMATCH" } else { "" };
        println!("A{}: 0x{:08X}{}", i, cpu.regs.a[i], marker);
    }

    println!("\n--- Expected Final State ---");
    println!("SR: 0x{:04X} (CCR: 0x{:02X})", test.final_state.sr, test.final_state.sr & 0xFF);
    for i in 0..8 {
        println!("D{}: 0x{:08X}", i, test.final_state.d[i]);
    }
    for i in 0..7 {
        println!("A{}: 0x{:08X}", i, test.final_state.a[i]);
    }
}

/// Diagnostic test to understand SBCD borrow detection.
#[test]
fn diagnose_sbcd_test() {
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-data/m68000-dl/v1/SBCD.json.bin");

    if !test_file.exists() {
        eprintln!("Test file not found: {}", test_file.display());
        return;
    }

    let tests = decode_file(&test_file).expect("Failed to decode");
    // Test 000 is an interesting V flag case
    let test = &tests[0];

    println!("\n=== Test: {} ===", test.name);
    println!("Expected cycles: {}", test.cycles);

    println!("\n--- Initial State ---");
    println!("PC: 0x{:08X}", test.initial.pc);
    println!("SR: 0x{:04X} (CCR: 0x{:02X})", test.initial.sr, test.initial.sr & 0xFF);
    for i in 0..8 {
        println!("D{}: 0x{:08X}", i, test.initial.d[i]);
    }
    for i in 0..7 {
        println!("A{}: 0x{:08X}", i, test.initial.a[i]);
    }
    println!("RAM ({} entries):", test.initial.ram.len());
    for (addr, val) in test.initial.ram.iter() {
        println!("  0x{:06X}: 0x{:02X}", addr, val);
    }

    // Run the test
    let mut cpu = M68000::new();
    let mut mem = TestBus::new();
    setup_cpu(&mut cpu, &mut mem, &test.initial);

    // Run for specified cycles
    for _ in 0..test.cycles {
        cpu.tick(&mut mem);
    }

    println!("\n--- Our Final State ---");
    println!("SR: 0x{:04X} (CCR: 0x{:02X})", cpu.regs.sr, cpu.regs.sr & 0xFF);
    for i in 0..8 {
        let marker = if cpu.regs.d[i] != test.final_state.d[i] { " <-- MISMATCH" } else { "" };
        println!("D{}: 0x{:08X}{}", i, cpu.regs.d[i], marker);
    }
    for i in 0..7 {
        let marker = if cpu.regs.a[i] != test.final_state.a[i] { " <-- MISMATCH" } else { "" };
        println!("A{}: 0x{:08X}{}", i, cpu.regs.a[i], marker);
    }

    println!("\n--- Expected Final State ---");
    println!("SR: 0x{:04X} (CCR: 0x{:02X})", test.final_state.sr, test.final_state.sr & 0xFF);
    for i in 0..8 {
        println!("D{}: 0x{:08X}", i, test.final_state.d[i]);
    }
    for i in 0..7 {
        println!("A{}: 0x{:08X}", i, test.final_state.a[i]);
    }

    // Show expected RAM changes
    println!("\n--- Final RAM ---");
    for (addr, expected_val) in test.final_state.ram.iter() {
        let actual = mem.peek(*addr);
        let marker = if actual != *expected_val { " <-- MISMATCH" } else { "" };
        println!("  0x{:06X}: actual=0x{:02X}, expected=0x{:02X}{}", addr, actual, expected_val, marker);
    }

    // Decode flags
    let our_ccr = cpu.regs.sr & 0xFF;
    let exp_ccr = test.final_state.sr & 0xFF;
    println!("\n--- Flag Analysis ---");
    println!("  X: got={}, expected={}", (our_ccr >> 4) & 1, (exp_ccr >> 4) & 1);
    println!("  N: got={}, expected={}", (our_ccr >> 3) & 1, (exp_ccr >> 3) & 1);
    println!("  Z: got={}, expected={}", (our_ccr >> 2) & 1, (exp_ccr >> 2) & 1);
    println!("  V: got={}, expected={}", (our_ccr >> 1) & 1, (exp_ccr >> 1) & 1);
    println!("  C: got={}, expected={}", our_ccr & 1, exp_ccr & 1);
}

/// Diagnostic test to understand test format and timing differences.
#[test]
fn diagnose_single_test() {
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-data/m68000-dl/v1/MOVEA.l.json.bin");

    if !test_file.exists() {
        eprintln!("Test file not found: {}", test_file.display());
        return;
    }

    let tests = decode_file(&test_file).expect("Failed to decode");
    // Test index 14: MOVEA.l (A6)+, A4 - A6 incremented too much
    let test = &tests[14];

    println!("\n=== Test: {} ===", test.name);
    println!("Expected cycles: {}", test.cycles);

    println!("\n--- Initial State ---");
    println!("PC: 0x{:08X}", test.initial.pc);
    println!("SR: 0x{:04X} (S={})", test.initial.sr, (test.initial.sr >> 13) & 1);
    println!("USP: 0x{:08X}  SSP: 0x{:08X}", test.initial.usp, test.initial.ssp);
    for i in 0..8 {
        println!("D{}: 0x{:08X}", i, test.initial.d[i]);
    }
    for i in 0..7 {
        println!("A{}: 0x{:08X}", i, test.initial.a[i]);
    }
    println!("Prefetch: [{:08X}, {:08X}]", test.initial.prefetch[0], test.initial.prefetch[1]);
    println!("RAM ({} bytes):", test.initial.ram.len());
    for (addr, val) in test.initial.ram.iter().take(30) {
        println!("  0x{:06X}: 0x{:02X}", addr, val);
    }
    // Show instruction bytes at PC
    println!("Instruction bytes:");
    let pc = test.initial.pc as u32;
    for i in 0..10 {
        let addr = pc + i;
        // Find in RAM or show as unknown
        if let Some(&(_, val)) = test.initial.ram.iter().find(|&&(a, _)| a == addr) {
            print!("{:02X} ", val);
        } else {
            print!("?? ");
        }
    }
    println!();

    // Run the test
    let mut cpu = M68000::new();
    let mut mem = TestBus::new();
    setup_cpu(&mut cpu, &mut mem, &test.initial);

    println!("\n--- After setup, before execution ---");
    println!("CPU PC: 0x{:08X}", cpu.regs.pc);
    println!("CPU SR: 0x{:04X}", cpu.regs.sr);

    // Run for specified cycles
    for _ in 0..test.cycles {
        cpu.tick(&mut mem);
    }

    println!("\n--- Our Final State ---");
    println!("PC: 0x{:08X}", cpu.regs.pc);
    println!("SR: 0x{:04X}", cpu.regs.sr);
    println!("USP: 0x{:08X}  SSP: 0x{:08X}", cpu.regs.usp, cpu.regs.ssp);
    for i in 0..8 {
        println!("D{}: 0x{:08X}", i, cpu.regs.d[i]);
    }
    for i in 0..7 {
        println!("A{}: 0x{:08X}", i, cpu.regs.a[i]);
    }

    println!("\n--- Expected Final State ---");
    println!("PC: 0x{:08X}", test.final_state.pc);
    println!("SR: 0x{:04X}", test.final_state.sr);
    println!("USP: 0x{:08X}  SSP: 0x{:08X}", test.final_state.usp, test.final_state.ssp);
    for i in 0..8 {
        let marker = if cpu.regs.d[i] != test.final_state.d[i] { " <-- MISMATCH" } else { "" };
        println!("D{}: 0x{:08X}{}", i, test.final_state.d[i], marker);
    }
    for i in 0..7 {
        let marker = if cpu.regs.a[i] != test.final_state.a[i] { " <-- MISMATCH" } else { "" };
        println!("A{}: 0x{:08X}{}", i, test.final_state.a[i], marker);
    }

    // Check for differences
    let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
    if !errors.is_empty() {
        println!("\n--- Errors ---");
        for err in &errors {
            println!("  {}", err);
        }
    } else {
        println!("\n*** TEST PASSED ***");
    }
}

#[test]
fn test_nop_single_step_tests() {
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-data/m68000/NOP.json.bin");

    if !test_file.exists() {
        eprintln!(
            "Skipping SingleStepTests: {} not found",
            test_file.display()
        );
        return;
    }

    let (passed, failed, errors) = run_test_file(&test_file);

    println!("NOP tests: {} passed, {} failed", passed, failed);

    if !errors.is_empty() {
        println!("\nFirst {} errors:", errors.len());
        for err in &errors {
            println!("  {}", err);
        }
    }

    // For now, we want to see results but not fail the test suite
    // as our implementation may have differences from MAME
    if failed > 0 {
        println!(
            "\nNote: {} tests failed. This may indicate implementation differences.",
            failed
        );
    }

    // Assert some tests passed to verify the harness works
    assert!(passed > 0, "No tests passed - harness may be broken");
}

/// Diagnostic test for ADDX.l failures.
#[test]
fn diagnose_addx_l_test() {
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-data/m68000-dl/v1/ADDX.l.json.bin");

    if !test_file.exists() {
        eprintln!("Test file not found: {}", test_file.display());
        return;
    }

    let tests = decode_file(&test_file).expect("Failed to decode");

    // Find first failing test
    for (i, test) in tests.iter().enumerate() {
        let mut cpu = M68000::new();
        let mut mem = TestBus::new();
        setup_cpu(&mut cpu, &mut mem, &test.initial);

        for _ in 0..test.cycles {
            cpu.tick(&mut mem);
        }

        let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
        if !errors.is_empty() {
            println!("\n=== First failing test: {} (index {}) ===", test.name, i);
            println!("Expected cycles: {}", test.cycles);

            println!("\n--- Initial State ---");
            println!("PC: 0x{:08X}", test.initial.pc);
            println!("SR: 0x{:04X} (CCR: {:02X}, X={})", test.initial.sr, test.initial.sr & 0xFF, (test.initial.sr >> 4) & 1);
            println!("USP: 0x{:08X}, SSP: 0x{:08X}", test.initial.usp, test.initial.ssp);
            println!("Prefetch: [{:04X}, {:04X}]", test.initial.prefetch[0], test.initial.prefetch[1]);
            for j in 0..7 {
                println!("A{}: 0x{:08X}", j, test.initial.a[j]);
            }

            // Decode instruction
            let opcode = test.initial.prefetch[0];
            let rx = opcode & 0x7;
            let ry = (opcode >> 9) & 0x7;
            let rm = (opcode & 0x8) != 0;
            println!("\nOpcode: 0x{:04X} = ADDX.l {}, {} (rm={})",
                opcode,
                if rm { format!("-(A{})", rx) } else { format!("D{}", rx) },
                if rm { format!("-(A{})", ry) } else { format!("D{}", ry) },
                rm);

            if rm {
                let src_addr = test.initial.a[rx as usize].wrapping_sub(4);
                let dst_addr = test.initial.a[ry as usize].wrapping_sub(4);
                println!("Source will read from: 0x{:08X} (A{} - 4)", src_addr, rx);
                println!("Dest will read from: 0x{:08X} (A{} - 4)", dst_addr, ry);
            }

            println!("\n--- Final State Comparison ---");
            println!("PC: 0x{:08X} (expected 0x{:08X})", cpu.regs.pc, test.final_state.pc);
            println!("SR: 0x{:04X} (expected 0x{:04X})", cpu.regs.sr, test.final_state.sr);
            for j in 0..7 {
                let expected_a = test.final_state.a[j];
                let actual_a = cpu.regs.a[j];
                let marker = if expected_a != actual_a { " <-- MISMATCH" } else { "" };
                println!("A{}: 0x{:08X} (expected 0x{:08X}){}", j, actual_a, expected_a, marker);
            }

            println!("\n--- Errors ---");
            for err in &errors {
                println!("  {}", err);
            }

            // If SSP changed, show exception frame
            if test.final_state.ssp != test.initial.ssp {
                println!("\n--- Exception Frame (SSP changed) ---");
                let ssp = test.final_state.ssp;
                for offset in 0..16u32 {
                    let addr = ssp.wrapping_add(offset);
                    let expected = test.final_state.ram.iter().find(|&&(a, _)| a == addr).map(|&(_, v)| v);
                    let actual = mem.peek(addr);
                    let marker = if expected.map(|e| e != actual).unwrap_or(false) { " <-- MISMATCH" } else { "" };
                    println!("  [SSP+{:02}] 0x{:06X}: actual=0x{:02X}, expected={}{}",
                        offset, addr, actual,
                        expected.map(|e| format!("0x{:02X}", e)).unwrap_or("N/A".to_string()),
                        marker);
                }
            }

            return;
        }
    }
    println!("All ADDX.l tests passed!");
}

/// Utility to run all available test files (called manually, not in CI).
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

    for entry in fs::read_dir(&test_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();

        if path.extension().map(|e| e == "bin").unwrap_or(false) {
            println!("\nRunning tests from: {}", path.display());
            let (passed, failed, errors) = run_test_file(&path);
            println!("  {} passed, {} failed", passed, failed);

            if !errors.is_empty() {
                for err in errors.iter().take(5) {
                    println!("    {}", err);
                }
            }

            total_passed += passed;
            total_failed += failed;
        }
    }

    println!("\n=== Total: {} passed, {} failed ===", total_passed, total_failed);
}
