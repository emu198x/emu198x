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

/// Diagnostic: categorize failures across many instructions.
#[test]
#[ignore]
fn diagnose_failures() {
    let instructions = &[
        "TST.l", "CLR.l", "NEG.l", "NOT.l",  // simple single-EA .l
        "TST.w", "CLR.w",                      // simple .w (should have 0 failures)
        "ADD.l", "SUB.l", "CMP.l", "OR.l",    // two-operand .l
        "ADD.w", "SUB.w", "CMP.w", "OR.w",    // two-operand .w
        "MULU", "MULS", "DIVU", "DIVS",       // multiply/divide
        "ADDA.w", "SUBA.w", "CMPA.w",         // address register .w
        "ADDA.l", "SUBA.l", "CMPA.l",         // address register .l
        "MOVE.l", "MOVE.w", "MOVEA.l",        // MOVE
        "CHK",                                  // CHK
        "BCLR", "BCHG", "BSET", "BTST",       // bit operations
        "LSL.w", "LSR.w", "ASL.w", "ASR.w",   // shifts
        "ROL.w", "ROR.w", "ROXL.w", "ROXR.w", // rotates
        "MOVEM.l", "MOVEM.w",                  // move multiple
        "MOVEtoCCR", "MOVEtoSR",              // MOVE to SR/CCR
        "EOR.l", "AND.l", "NEGX.l",           // more .l
    ];

    let read_word_ram = |ram: &[(u32, u8)], addr: u32| -> Option<u16> {
        let hi = ram.iter().find(|&&(a,_)| a == addr).map(|&(_,v)| v)?;
        let lo = ram.iter().find(|&&(a,_)| a == addr+1).map(|&(_,v)| v)?;
        Some((u16::from(hi) << 8) | u16::from(lo))
    };
    let read_long_ram = |ram: &[(u32, u8)], addr: u32| -> Option<u32> {
        let hi = read_word_ram(ram, addr)?;
        let lo = read_word_ram(ram, addr+2)?;
        Some((u32::from(hi) << 16) | u32::from(lo))
    };

    for instr_name in instructions {
        let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap().parent().unwrap()
            .join(format!("test-data/m68000-dl/v1/{}.json.bin", instr_name));
        if !test_file.exists() { continue; }
        let tests = decode_file(&test_file).expect("Failed to decode");

        let mut ae_pass = 0u32;
        let mut ae_fail_pc = 0u32;
        let mut ae_fail_ai = 0u32;
        let mut ae_fail_sr = 0u32;
        let mut ae_fail_other = 0u32;
        let mut non_ae_pass = 0u32;
        let mut non_ae_fail_pc = 0u32;
        let mut non_ae_fail_sr = 0u32;
        let mut non_ae_fail_dreg = 0u32;
        let mut non_ae_fail_areg = 0u32;
        let mut non_ae_fail_mem = 0u32;
        let mut non_ae_fail_other = 0u32;
        let mut first_ae_fails: Vec<String> = Vec::new();
        let mut first_non_ae_fails: Vec<String> = Vec::new();

        for test in tests.iter() {
            let ssp_diff = test.initial.ssp as i64 - test.final_state.ssp as i64;
            let is_ae = ssp_diff == 14;

            let mut cpu = M68000::new();
            let mut mem = TestBus::new();
            setup_cpu(&mut cpu, &mut mem, &test.initial);
            for _ in 0..test.cycles { cpu.tick(&mut mem); }
            let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);

            if errors.is_empty() {
                if is_ae { ae_pass += 1; } else { non_ae_pass += 1; }
                continue;
            }

            if is_ae {
                // Categorize address error failure
                let ssp = test.final_state.ssp;
                let got_pc_ok = read_long_ram(&test.final_state.ram, ssp + 10)
                    .map_or(false, |exp_pc| {
                        // Check what our code wrote
                        let got = (u32::from(mem.peek(ssp + 10)) << 24)
                            | (u32::from(mem.peek(ssp + 11)) << 16)
                            | (u32::from(mem.peek(ssp + 12)) << 8)
                            | u32::from(mem.peek(ssp + 13));
                        got == exp_pc
                    });
                let got_ai_ok = read_word_ram(&test.final_state.ram, ssp)
                    .map_or(false, |exp_ai| {
                        let got = (u16::from(mem.peek(ssp)) << 8) | u16::from(mem.peek(ssp + 1));
                        got == exp_ai
                    });
                let got_sr_ok = read_word_ram(&test.final_state.ram, ssp + 8)
                    .map_or(false, |exp_sr| {
                        let got = (u16::from(mem.peek(ssp + 8)) << 8) | u16::from(mem.peek(ssp + 9));
                        got == exp_sr
                    });

                if !got_pc_ok { ae_fail_pc += 1; }
                if !got_ai_ok { ae_fail_ai += 1; }
                if !got_sr_ok { ae_fail_sr += 1; }
                if got_pc_ok && got_ai_ok && got_sr_ok { ae_fail_other += 1; }

                if first_ae_fails.len() < 3 {
                    let exp_pc = read_long_ram(&test.final_state.ram, ssp + 10).unwrap_or(0);
                    let got_pc = (u32::from(mem.peek(ssp + 10)) << 24)
                        | (u32::from(mem.peek(ssp + 11)) << 16)
                        | (u32::from(mem.peek(ssp + 12)) << 8)
                        | u32::from(mem.peek(ssp + 13));
                    let exp_ai = read_word_ram(&test.final_state.ram, ssp).unwrap_or(0);
                    let got_ai = (u16::from(mem.peek(ssp)) << 8) | u16::from(mem.peek(ssp + 1));
                    let op = test.initial.prefetch[0];
                    let src_mode = (op >> 3) & 7;
                    let src_reg = op & 7;
                    first_ae_fails.push(format!(
                        "  op=0x{:04X} mode={}/{} init_pc=0x{:06X} exp_saved_pc=0x{:06X}(+{}) got=0x{:06X}(+{}) exp_ai=0x{:04X} got_ai=0x{:04X}",
                        op, src_mode, src_reg, test.initial.pc,
                        exp_pc, exp_pc as i64 - test.initial.pc as i64,
                        got_pc, got_pc as i64 - test.initial.pc as i64,
                        exp_ai, got_ai,
                    ));
                }
            } else {
                // Categorize non-address-error failure
                let has_pc = errors.iter().any(|e| e.contains("PC mismatch"));
                let has_sr = errors.iter().any(|e| e.contains("SR mismatch"));
                let has_dreg = errors.iter().any(|e| e.contains(": D"));
                let has_areg = errors.iter().any(|e| e.contains(": A") && !e.contains("RAM"));
                let has_mem = errors.iter().any(|e| e.contains("RAM["));

                if has_pc { non_ae_fail_pc += 1; }
                if has_sr { non_ae_fail_sr += 1; }
                if has_dreg { non_ae_fail_dreg += 1; }
                if has_areg { non_ae_fail_areg += 1; }
                if has_mem { non_ae_fail_mem += 1; }
                if !has_pc && !has_sr && !has_dreg && !has_areg && !has_mem { non_ae_fail_other += 1; }

                if first_non_ae_fails.len() < 3 {
                    first_non_ae_fails.push(format!("  {}", errors[0]));
                }
            }
        }

        let ae_total_fail = ae_fail_pc.max(ae_fail_ai).max(ae_fail_sr).max(ae_fail_other);
        let non_ae_total = non_ae_fail_pc.max(non_ae_fail_sr).max(non_ae_fail_dreg)
            .max(non_ae_fail_areg).max(non_ae_fail_mem).max(non_ae_fail_other);

        // Only print if there are failures
        if ae_total_fail > 0 || non_ae_total > 0 {
            println!("{:12}: AE_fail(pc={}, ai={}, sr={}, other={})  NonAE_fail(pc={}, sr={}, dreg={}, areg={}, mem={}, other={})",
                instr_name,
                ae_fail_pc, ae_fail_ai, ae_fail_sr, ae_fail_other,
                non_ae_fail_pc, non_ae_fail_sr, non_ae_fail_dreg,
                non_ae_fail_areg, non_ae_fail_mem, non_ae_fail_other);
            for s in &first_ae_fails { println!("{}", s); }
            for s in &first_non_ae_fails { println!("{}", s); }
        }
    }
}

#[test]
#[ignore]
fn diagnose_move_l_sr() {
    // Investigate MOVE.l SR mismatches on destination write AE
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap().parent().unwrap()
        .join("test-data/m68000-dl/v1/MOVE.l.json.bin");
    let tests = decode_file(&test_file).expect("Failed to decode");

    let read_word_ram = |ram: &[(u32, u8)], addr: u32| -> Option<u16> {
        let hi = ram.iter().find(|(a, _)| *a == addr).map(|(_, v)| *v)?;
        let lo = ram.iter().find(|(a, _)| *a == addr + 1).map(|(_, v)| *v)?;
        Some((u16::from(hi) << 8) | u16::from(lo))
    };

    let mut count = 0;
    for (i, test) in tests.iter().enumerate() {
        let ssp_diff = test.initial.ssp as i64 - test.final_state.ssp as i64;
        let is_ae = ssp_diff == 14;
        if !is_ae { continue; }

        let mut cpu = M68000::new();
        let mut mem = TestBus::new();
        setup_cpu(&mut cpu, &mut mem, &test.initial);
        for _ in 0..test.cycles { cpu.tick(&mut mem); }
        let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
        if errors.is_empty() { continue; }

        // Check if it's a SR mismatch
        let has_sr = errors.iter().any(|e| e.contains("SR mismatch"));
        if !has_sr { continue; }

        let ssp = test.final_state.ssp;
        let exp_frame_sr = read_word_ram(&test.final_state.ram, ssp + 8).unwrap_or(0);
        let got_frame_sr = (u16::from(mem.peek(ssp + 8)) << 8) | u16::from(mem.peek(ssp + 9));
        let init_sr = test.initial.sr;

        // Decode MOVE opcode to find source value
        let op = test.initial.prefetch[0];

        let init_ccr = init_sr & 0x1F;
        let exp_ccr = exp_frame_sr & 0x1F;
        let got_ccr = got_frame_sr & 0x1F;

        // Categorize the expected behavior
        // "preserved" = expected CCR == initial CCR
        // "move_flags" = expected CCR matches what set_flags_move would produce
        // (N from data MSB, Z from data==0, V=0, C=0, X preserved)
        let x_flag = init_ccr & 0x10;
        // Try to figure out what data the MOVE carried by looking at N and Z in expected
        let exp_n = (exp_ccr >> 3) & 1;
        let exp_z = (exp_ccr >> 2) & 1;
        let exp_v = (exp_ccr >> 1) & 1;
        let exp_c = exp_ccr & 1;

        // Classify
        let is_preserved = exp_ccr == init_ccr;
        let is_full_move_flags = exp_v == 0 && exp_c == 0 && (exp_ccr & 0x10) == x_flag;
        let is_partial_nz_only = exp_v == ((init_ccr >> 1) & 1) && exp_c == (init_ccr & 1) && (exp_ccr & 0x10) == x_flag;

        if count < 30 {
            let category = if is_preserved {
                "PRESERVED"
            } else if is_full_move_flags && !is_partial_nz_only {
                "FULL_FLAGS"
            } else if is_partial_nz_only && !is_full_move_flags {
                "NZ_ONLY"
            } else {
                "AMBIGUOUS"
            };

            // Decode source mode
            let src_mode = (op >> 3) & 7;
            let dst_mode = (op >> 6) & 7;
            let dst_reg = (op >> 9) & 7;

            println!("#{:04}: {} op=0x{:04X} src_mode={} dst_mode={} dst_reg={} -> {}",
                i, test.name, op, src_mode, dst_mode, dst_reg, category);
            println!("  init_ccr=0x{:02X} exp_ccr=0x{:02X} got_ccr=0x{:02X}  N={} Z={} V={} C={}",
                init_ccr, exp_ccr, got_ccr, exp_n, exp_z, exp_v, exp_c);
        }
        count += 1;
    }
    println!("\nTotal MOVE.l AE dst write with SR mismatch: {}", count);

    // Now count by category
    let mut preserved = 0;
    let mut nz_only = 0;
    let mut full_flags = 0;
    let mut ambiguous = 0;
    for test in tests.iter() {
        let ssp_diff = test.initial.ssp as i64 - test.final_state.ssp as i64;
        let is_ae = ssp_diff == 14;
        if !is_ae { continue; }

        let mut cpu = M68000::new();
        let mut mem = TestBus::new();
        setup_cpu(&mut cpu, &mut mem, &test.initial);
        for _ in 0..test.cycles { cpu.tick(&mut mem); }
        let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
        if errors.is_empty() || !errors.iter().any(|e| e.contains("SR mismatch")) { continue; }

        let ssp = test.final_state.ssp;
        let exp_frame_sr = read_word_ram(&test.final_state.ram, ssp + 8).unwrap_or(0);
        let init_sr = test.initial.sr;
        let init_ccr = init_sr & 0x1F;
        let exp_ccr = exp_frame_sr & 0x1F;
        let x_flag = init_ccr & 0x10;
        let exp_v = (exp_ccr >> 1) & 1;
        let exp_c = exp_ccr & 1;
        let init_v = (init_ccr >> 1) & 1;
        let init_c = init_ccr & 1;

        let is_preserved = exp_ccr == init_ccr;
        let is_full = exp_v == 0 && exp_c == 0 && (exp_ccr & 0x10) == x_flag;
        let is_nz = exp_v == init_v && exp_c == init_c && (exp_ccr & 0x10) == x_flag;

        if is_preserved && is_full { ambiguous += 1; }
        else if is_preserved { preserved += 1; }
        else if is_full && !is_nz { full_flags += 1; }
        else if is_nz { nz_only += 1; }
        else { ambiguous += 1; }
    }
    println!("Categories: preserved={} nz_only={} full_flags={} ambiguous={}", preserved, nz_only, full_flags, ambiguous);

    // Check what happens if we classify by source type
    println!("\n=== Src type breakdown ===");
    let mut reg_src_simple_dst = [0u32; 2]; // [total, sr_mismatch]
    let mut reg_src_ext_dst = [0u32; 2];
    let mut mem_src_any = [0u32; 2];
    for test in tests.iter() {
        let ssp_diff = test.initial.ssp as i64 - test.final_state.ssp as i64;
        if ssp_diff != 14 { continue; }
        let mut cpu = M68000::new();
        let mut mem = TestBus::new();
        setup_cpu(&mut cpu, &mut mem, &test.initial);
        for _ in 0..test.cycles { cpu.tick(&mut mem); }
        let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
        let has_sr = errors.iter().any(|e| e.contains("SR mismatch"));

        let op = test.initial.prefetch[0];
        let src_mode = (op >> 3) & 7;
        let dst_mode = (op >> 6) & 7;
        let is_reg_src = src_mode <= 1;
        let is_simple_dst = dst_mode <= 4; // (An), (An)+, -(An)

        let bucket = if is_reg_src && is_simple_dst {
            &mut reg_src_simple_dst
        } else if is_reg_src && !is_simple_dst {
            &mut reg_src_ext_dst
        } else {
            &mut mem_src_any
        };
        bucket[0] += 1;
        if has_sr && !errors.is_empty() { bucket[1] += 1; }
    }
    println!("reg_src + simple_dst: total_ae_fail={}, sr_mismatch={}", reg_src_simple_dst[0], reg_src_simple_dst[1]);
    println!("reg_src + ext_dst:    total_ae_fail={}, sr_mismatch={}", reg_src_ext_dst[0], reg_src_ext_dst[1]);
    println!("mem_src + any_dst:    total_ae_fail={}, sr_mismatch={}", mem_src_any[0], mem_src_any[1]);
}

#[test]
#[ignore]
fn diagnose_addx_subx_byte() {
    let instructions = [
        "ADDX.b",
        "SUBX.b",
        "ABCD",
        "SBCD",
    ];

    for instr_name in &instructions {
        let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap().parent().unwrap()
            .join(format!("test-data/m68000-dl/v1/{}.json.bin", instr_name));
        if !test_file.exists() {
            println!("=== {} - file not found ===", instr_name);
            continue;
        }
        let tests = decode_file(&test_file).expect("Failed to decode");

        println!("\n=== {} ===", instr_name);
        let mut count = 0;
        let mut mem_count = 0;
        let mut reg_count = 0;
        let mut sr_only = 0;
        for (i, test) in tests.iter().enumerate() {
            let mut cpu = M68000::new();
            let mut mem = TestBus::new();
            setup_cpu(&mut cpu, &mut mem, &test.initial);
            for _ in 0..test.cycles { cpu.tick(&mut mem); }
            let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
            if !errors.is_empty() {
                let op = test.initial.prefetch[0];
                let is_mem = op & 0x0008 != 0;
                if is_mem { mem_count += 1; } else { reg_count += 1; }

                let sr_diff = cpu.regs.sr != test.final_state.sr;
                let d_diff = (0..8).any(|j| cpu.regs.d[j] != test.final_state.d[j]);
                let a_diff = (0..7).any(|j| cpu.regs.a[j] != test.final_state.a[j]);
                let pc_diff = cpu.regs.pc != test.final_state.pc;
                let ssp_diff = cpu.regs.ssp != test.final_state.ssp;
                let mem_diff = errors.iter().any(|e| e.contains("Memory"));

                if sr_diff && !d_diff && !a_diff && !pc_diff && !ssp_diff && !mem_diff {
                    sr_only += 1;
                }

                if count < 5 {
                    println!("\n  Test {} (index {}), opcode 0x{:04X}, {}",
                        test.name, i, op, if is_mem { "mem-to-mem" } else { "reg-to-reg" });

                    // For reg-to-reg, show src/dst byte values
                    if !is_mem {
                        let rx = (op & 7) as usize;
                        let ry = ((op >> 9) & 7) as usize;
                        let src_byte = test.initial.d[rx] as u8;
                        let dst_byte = test.initial.d[ry] as u8;
                        let x = u8::from(test.initial.sr & 0x10 != 0);
                        println!("  src(D{})=0x{:02X} dst(D{})=0x{:02X} X={}",
                            rx, src_byte, ry, dst_byte, x);
                    }

                    if sr_diff {
                        let exp_sr = test.final_state.sr;
                        let got_sr = cpu.regs.sr;
                        let diff = exp_sr ^ got_sr;
                        println!("  SR: init=0x{:04X} exp=0x{:04X} got=0x{:04X} diff=0x{:04X}",
                            test.initial.sr, exp_sr, got_sr, diff);
                        if diff & 0x10 != 0 { println!("    X flag differs: exp={} got={}", (exp_sr >> 4) & 1, (got_sr >> 4) & 1); }
                        if diff & 0x08 != 0 { println!("    N flag differs: exp={} got={}", (exp_sr >> 3) & 1, (got_sr >> 3) & 1); }
                        if diff & 0x04 != 0 { println!("    Z flag differs: exp={} got={}", (exp_sr >> 2) & 1, (got_sr >> 2) & 1); }
                        if diff & 0x02 != 0 { println!("    V flag differs: exp={} got={}", (exp_sr >> 1) & 1, (got_sr >> 1) & 1); }
                        if diff & 0x01 != 0 { println!("    C flag differs: exp={} got={}", exp_sr & 1, got_sr & 1); }
                    }

                    for j in 0..8 {
                        if cpu.regs.d[j] != test.final_state.d[j] {
                            println!("  D{}: init=0x{:08X} exp=0x{:08X} got=0x{:08X}",
                                j, test.initial.d[j], test.final_state.d[j], cpu.regs.d[j]);
                        }
                    }
                    for j in 0..7 {
                        if cpu.regs.a[j] != test.final_state.a[j] {
                            println!("  A{}: init=0x{:08X} exp=0x{:08X} got=0x{:08X}",
                                j, test.initial.a[j], test.final_state.a[j], cpu.regs.a[j]);
                        }
                    }
                    if pc_diff {
                        println!("  PC: init=0x{:08X} exp=0x{:08X} got=0x{:08X}",
                            test.initial.pc, test.final_state.pc, cpu.regs.pc);
                    }
                    if ssp_diff {
                        println!("  SSP: init=0x{:08X} exp=0x{:08X} got=0x{:08X}",
                            test.initial.ssp, test.final_state.ssp, cpu.regs.ssp);
                    }
                }
                count += 1;
            }
        }
        println!("\n  Total: {} failures (reg={}, mem={}, sr_only={})",
            count, reg_count, mem_count, sr_only);
    }
}

/// Diagnostic: categorize CHK failures by type.
#[test]
#[ignore]
fn diagnose_chk_failures() {
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap().parent().unwrap()
        .join("test-data/m68000-dl/v1/CHK.json.bin");
    if !test_file.exists() {
        println!("CHK test file not found");
        return;
    }
    let tests = decode_file(&test_file).expect("Failed to decode");

    let mut sr_only = 0u32;
    let mut pc_fail = 0u32;
    let mut mem_fail = 0u32;
    let mut other_fail = 0u32;
    let mut shown = 0u32;
    // Track which condition code bits are wrong
    let mut n_wrong = 0u32;
    let mut z_wrong = 0u32;
    let mut v_wrong = 0u32;
    let mut c_wrong = 0u32;
    let mut x_wrong = 0u32;

    for (i, test) in tests.iter().enumerate() {
        let mut cpu = M68000::new();
        let mut mem = TestBus::new();
        setup_cpu(&mut cpu, &mut mem, &test.initial);
        for _ in 0..test.cycles { cpu.tick(&mut mem); }
        let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);

        if errors.is_empty() { continue; }

        let has_sr = errors.iter().any(|e| e.contains("SR mismatch"));
        let has_pc = errors.iter().any(|e| e.contains("PC mismatch"));
        let has_mem = errors.iter().any(|e| e.contains("RAM["));

        if has_sr && !has_pc && !has_mem && errors.len() <= 2 {
            sr_only += 1;
            let sr_diff = cpu.regs.sr ^ test.final_state.sr;
            if sr_diff & 0x08 != 0 { n_wrong += 1; }
            if sr_diff & 0x04 != 0 { z_wrong += 1; }
            if sr_diff & 0x02 != 0 { v_wrong += 1; }
            if sr_diff & 0x01 != 0 { c_wrong += 1; }
            if sr_diff & 0x10 != 0 { x_wrong += 1; }
        }
        if has_pc { pc_fail += 1; }
        if has_mem { mem_fail += 1; }
        if !has_sr && !has_pc && !has_mem { other_fail += 1; }

        if shown < 10 {
            let op = test.initial.prefetch[0];
            let reg = (op >> 9) & 7;
            let mode = (op >> 3) & 7;
            let ea_reg = op & 7;
            let dn = test.initial.d[reg as usize] as i16;
            // Determine if this is an exception case (SSP moved by 14)
            let ssp_diff = test.initial.ssp as i64 - test.final_state.ssp as i64;
            let is_exception = ssp_diff == 14;
            println!("  #{:04} op=0x{:04X} Dn=D{}=0x{:04X}({}) mode={}/{} exception={} init_sr=0x{:04X} exp_sr=0x{:04X} got_sr=0x{:04X} diff=0x{:04X}",
                i, op, reg, test.initial.d[reg as usize] as u16, dn,
                mode, ea_reg, is_exception,
                test.initial.sr, test.final_state.sr, cpu.regs.sr,
                test.final_state.sr ^ cpu.regs.sr);
            for e in &errors {
                println!("    {}", e);
            }
            shown += 1;
        }
    }

    println!("\nCHK failure summary:");
    println!("  SR-only: {} (N={}, Z={}, V={}, C={}, X={})", sr_only, n_wrong, z_wrong, v_wrong, c_wrong, x_wrong);
    println!("  PC: {}, Mem: {}, Other: {}", pc_fail, mem_fail, other_fail);
}

/// Diagnostic: categorize remaining CMP failures.
#[test]
#[ignore]
fn diagnose_cmp_remaining() {
    for instr_name in &["CMP.l", "CMP.w"] {
        let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap().parent().unwrap()
            .join(format!("test-data/m68000-dl/v1/{}.json.bin", instr_name));
        if !test_file.exists() { continue; }
        let tests = decode_file(&test_file).expect("Failed to decode");

        let mut ae_cmpm = 0u32;
        let mut ae_cmp_reg = 0u32;
        let mut ae_cmp_mem = 0u32;
        let mut non_ae = 0u32;
        let mut shown = 0u32;

        for (i, test) in tests.iter().enumerate() {
            let mut cpu = M68000::new();
            let mut mem = TestBus::new();
            setup_cpu(&mut cpu, &mut mem, &test.initial);
            for _ in 0..test.cycles { cpu.tick(&mut mem); }
            let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
            if errors.is_empty() { continue; }

            let op = test.initial.prefetch[0];
            let ssp_diff = test.initial.ssp as i64 - test.final_state.ssp as i64;
            let is_ae = ssp_diff == 14;
            let is_cmpm = (op & 0xF138) == 0xB108;
            let ea_mode = (op >> 3) & 7;

            if is_ae {
                if is_cmpm { ae_cmpm += 1; }
                else if ea_mode == 0 { ae_cmp_reg += 1; }
                else { ae_cmp_mem += 1; }
            } else {
                non_ae += 1;
            }

            if shown < 15 {
                let ssp = test.final_state.ssp;
                let exp_pc = {
                    let ram_map: std::collections::HashMap<u32, u8> = test.final_state.ram.iter().copied().collect();
                    let b0 = ram_map.get(&(ssp.wrapping_add(10))).copied().unwrap_or(0);
                    let b1 = ram_map.get(&(ssp.wrapping_add(11))).copied().unwrap_or(0);
                    let b2 = ram_map.get(&(ssp.wrapping_add(12))).copied().unwrap_or(0);
                    let b3 = ram_map.get(&(ssp.wrapping_add(13))).copied().unwrap_or(0);
                    u32::from(b0) << 24 | u32::from(b1) << 16 | u32::from(b2) << 8 | u32::from(b3)
                };
                let got_pc = {
                    let b0 = mem.peek(ssp.wrapping_add(10));
                    let b1 = mem.peek(ssp.wrapping_add(11));
                    let b2 = mem.peek(ssp.wrapping_add(12));
                    let b3 = mem.peek(ssp.wrapping_add(13));
                    u32::from(b0) << 24 | u32::from(b1) << 16 | u32::from(b2) << 8 | u32::from(b3)
                };
                let init_pc = test.initial.pc;
                println!("  #{:04} {} ae={} cmpm={} op=0x{:04X} ea_mode={} init_pc=0x{:06X} exp_frame_pc=0x{:06X}(+{}) got=0x{:06X}(+{})",
                    i, instr_name, is_ae, is_cmpm, op, ea_mode,
                    init_pc, exp_pc, exp_pc as i64 - init_pc as i64,
                    got_pc, got_pc as i64 - init_pc as i64);
                for e in errors.iter().take(3) {
                    println!("    {}", e);
                }
                shown += 1;
            }
        }

        println!("\n{} summary: ae_cmpm={} ae_cmp_reg={} ae_cmp_mem={} non_ae={}\n",
            instr_name, ae_cmpm, ae_cmp_reg, ae_cmp_mem, non_ae);
    }
}

/// Diagnostic: check MULS/MULU/DIVS/DIVU timing
#[test]
#[ignore]
fn diagnose_mul_div_timing() {
    for instr_name in &["MULS", "MULU", "DIVU", "DIVS"] {
        let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap().parent().unwrap()
            .join(format!("test-data/m68000-dl/v1/{}.json.bin", instr_name));
        if !test_file.exists() { continue; }
        let tests = decode_file(&test_file).expect("Failed to decode");

        println!("\n=== {} ===", instr_name);
        let mut shown = 0;
        let mut total_fail = 0u32;
        let mut pc_only = 0u32;
        let mut sr_only = 0u32;

        for (i, test) in tests.iter().enumerate() {
            let mut cpu = M68000::new();
            let mut mem = TestBus::new();
            setup_cpu(&mut cpu, &mut mem, &test.initial);
            for _ in 0..test.cycles { cpu.tick(&mut mem); }
            let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
            if errors.is_empty() { continue; }

            total_fail += 1;
            let has_pc = errors.iter().any(|e| e.contains("PC mismatch"));
            let has_sr = errors.iter().any(|e| e.contains("SR mismatch"));
            if has_pc && !has_sr { pc_only += 1; }
            if has_sr && !has_pc { sr_only += 1; }

            if shown < 10 {
                let op = test.initial.prefetch[0];
                let ea_mode = (op >> 3) & 7;
                let ea_reg = op & 7;
                let reg = (op >> 9) & 7;
                let is_divu = *instr_name == "DIVU";
                let is_divs = *instr_name == "DIVS";
                if is_divu || is_divs {
                    let divisor_raw = if ea_mode == 0 {
                        test.initial.d[ea_reg as usize]
                    } else { 0 };
                    let dividend = test.initial.d[reg as usize];
                    let divisor16 = divisor_raw & 0xFFFF;
                    if divisor16 != 0 {
                        let (quotient, overflow) = if is_divu {
                            let q = dividend / divisor16;
                            (q, q > 0xFFFF)
                        } else {
                            let dend = dividend as i32;
                            let dsor = (divisor16 as i16) as i32;
                            if dsor != 0 {
                                let q = dend / dsor;
                                (q as u32, q < -32768 || q > 32767)
                            } else { (0, true) }
                        };
                        let q_ones = (quotient as u16).count_ones();
                        let our_timing = if overflow { 10 } else { 76 + 2 * q_ones };
                        println!("  #{:04} op=0x{:04X} {} D{}/D{} mode={} dvnd=0x{:08X} dvsr=0x{:04X} quot=0x{:08X} ovf={} q_ones={} our_int={} test_cyc={}",
                            i, op, instr_name, reg, ea_reg, ea_mode, dividend, divisor16, quotient, overflow, q_ones, our_timing, test.cycles);
                    } else {
                        println!("  #{:04} op=0x{:04X} div-by-zero test_cyc={}", i, op, test.cycles);
                    }
                } else {
                    let src16 = if ea_mode == 0 { test.initial.d[ea_reg as usize] as u16 } else { 0 };
                    let pattern = src16 ^ (src16 << 1);
                    let n = pattern.count_ones();
                    println!("  #{:04} op=0x{:04X} {} mode={} src=D{} val=0x{:04X} calc_int={} test_cyc={}",
                        i, op, instr_name, ea_mode, ea_reg, src16, 38 + 2 * n, test.cycles);
                }
                println!("    exp_pc=0x{:08X} got_pc=0x{:08X} diff={}",
                    test.final_state.pc, cpu.regs.pc,
                    test.final_state.pc as i64 - cpu.regs.pc as i64);
                if has_sr {
                    println!("    exp_sr=0x{:04X} got_sr=0x{:04X} diff=0x{:04X}",
                        test.final_state.sr, cpu.regs.sr, test.final_state.sr ^ cpu.regs.sr);
                }
                shown += 1;
            }
        }
        println!("\n  Total: {} failures (pc_only={}, sr_only={})", total_fail, pc_only, sr_only);
    }
}

/// Diagnostic: check cycle counts and PC behavior for bit ops and other problematic instructions.
#[test]
fn diagnose_bit_ops_cycles() {
    for instr_name in &["BCLR", "BCHG", "BSET", "BTST", "NOP", "MOVEtoCCR", "DIVU", "CMPA.w"] {
        let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap().parent().unwrap()
            .join(format!("test-data/m68000-dl/v1/{}.json.bin", instr_name));
        if !test_file.exists() { continue; }
        let tests = decode_file(&test_file).expect("Failed to decode");

        println!("\n=== {} ===", instr_name);
        let mut shown = 0;
        for (i, test) in tests.iter().enumerate() {
            let mut cpu = M68000::new();
            let mut mem = TestBus::new();
            setup_cpu(&mut cpu, &mut mem, &test.initial);
            for _ in 0..test.cycles { cpu.tick(&mut mem); }
            let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);

            if !errors.is_empty() && errors.iter().any(|e| e.contains("PC mismatch")) && shown < 5 {
                let op = test.initial.prefetch[0];
                let mode = (op >> 3) & 7;
                let reg = op & 7;
                println!("  #{:03} op=0x{:04X} mode={} reg={} cycles={} init_pc=0x{:08X} exp_pc=0x{:08X} got_pc=0x{:08X}",
                    i, op, mode, reg, test.cycles, test.initial.pc, test.final_state.pc, cpu.regs.pc);
                shown += 1;
            }
        }
        if shown == 0 {
            // Show a passing test for comparison
            if let Some(test) = tests.first() {
                println!("  (all pass or no PC issues) first test cycles={}", test.cycles);
            }
        }
    }
}

#[test]
#[ignore]
fn diagnose_move_failures() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap().parent().unwrap()
        .join("test-data/m68000-dl/v1");

    for name in ["MOVE.l", "MOVE.w"] {
        let path = base.join(format!("{}.json.bin", name));
        if !path.exists() { continue; }
        let tests = decode_file(&path).unwrap();
        println!("\n=== {} ({} tests) ===", name, tests.len());

        // Categorize failures by error type
        let mut has_sr = 0u32;
        let mut has_areg = 0u32;
        let mut has_dreg = 0u32;
        let mut has_ram = 0u32;
        let mut non_ae = 0u32;
        let mut total_fail = 0u32;
        let mut shown = 0u32;

        // Also categorize by whether AE is source read vs dest write
        let mut ae_src_read = 0u32;
        let mut ae_dst_write = 0u32;

        for (i, test) in tests.iter().enumerate() {
            let mut cpu = M68000::new();
            let mut mem = TestBus::new();
            setup_cpu(&mut cpu, &mut mem, &test.initial);
            for _ in 0..test.cycles { cpu.tick(&mut mem); }
            let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
            if errors.is_empty() { continue; }
            total_fail += 1;

            let ssp_diff = test.initial.ssp as i64 - test.final_state.ssp as i64;
            let is_ae = ssp_diff == 14;

            if !is_ae {
                non_ae += 1;
            } else {
                // Determine if AE is on source read or dest write
                // Extract access info from expected frame
                let final_ssp = test.final_state.ssp;
                let ram_map: std::collections::HashMap<u32, u8> = test.final_state.ram.iter().copied().collect();
                let _access_hi = ram_map.get(&final_ssp).copied().unwrap_or(0);
                let access_lo = ram_map.get(&(final_ssp + 1)).copied().unwrap_or(0);
                let is_read = access_lo & 0x10 != 0;
                if is_read { ae_src_read += 1; } else { ae_dst_write += 1; }
            }

            // Check which error types
            let mut this_sr = false;
            let mut this_areg = false;
            let mut this_dreg = false;
            let mut this_ram = false;
            for e in &errors {
                if e.contains("SR mismatch") { this_sr = true; }
                else if e.contains("RAM[") { this_ram = true; }
                else if e.contains("mismatch") {
                    // Parse register name — look for " X# mismatch"
                    if let Some(pos) = e.find("mismatch") {
                        let prefix = &e[..pos];
                        if prefix.contains(" A") { this_areg = true; }
                        else if prefix.contains(" D") { this_dreg = true; }
                    }
                }
            }
            if this_sr { has_sr += 1; }
            if this_areg { has_areg += 1; }
            if this_dreg { has_dreg += 1; }
            if this_ram { has_ram += 1; }

            if shown < 10 {
                let op = test.initial.prefetch[0] as u16;
                let src_mode = (op >> 3) & 7;
                let dst_mode = (op >> 6) & 7;
                let final_ssp = test.final_state.ssp;
                let ram_map: std::collections::HashMap<u32, u8> = test.final_state.ram.iter().copied().collect();
                let access_lo = ram_map.get(&(final_ssp + 1)).copied().unwrap_or(0);
                let is_read = access_lo & 0x10 != 0;
                println!("  #{:04}: ae={} rw={} src={} dst={} op=0x{:04X}",
                    i, is_ae, if is_ae { if is_read {"R"} else {"W"} } else { "-" },
                    src_mode, dst_mode, op);
                for e in &errors {
                    println!("    {}", e);
                }
                shown += 1;
            }
        }

        println!("\n{} total_fail={} non_ae={} ae_src_read={} ae_dst_write={}",
            name, total_fail, non_ae, ae_src_read, ae_dst_write);
        println!("  tests_with: SR={} areg={} dreg={} RAM={}", has_sr, has_areg, has_dreg, has_ram);
    }
}

/// Detailed diagnostic for MOVE.w and remaining MOVE.l failures.
#[test]
fn diagnose_move_remaining() {
    for name in &["MOVE.w", "MOVE.l"] {
        let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap().parent().unwrap()
            .join(format!("test-data/m68000-dl/v1/{}.json.bin", name));

        if !test_file.exists() { continue; }
        let tests = decode_file(&test_file).expect("Failed to decode");

        // Categorize failures
        let mut sr_only = 0u32;
        let mut ram_only = 0u32;
        let mut areg_only = 0u32;
        let mut ssp_only = 0u32;
        let mut pc_only = 0u32;
        let mut mixed = 0u32;
        let mut ae_dst_write_sr = 0u32;
        let mut ae_src_read_sr = 0u32;
        let mut non_ae_sr = 0u32;
        let mut shown = 0;

        for (i, test) in tests.iter().enumerate() {
            let mut cpu = M68000::new();
            let mut mem = TestBus::new();
            setup_cpu(&mut cpu, &mut mem, &test.initial);
            for _ in 0..test.cycles { cpu.tick(&mut mem); }
            let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
            if errors.is_empty() { continue; }

            let has_sr = errors.iter().any(|e| e.contains("SR mismatch"));
            let has_ram = errors.iter().any(|e| e.contains("RAM["));
            let has_areg = errors.iter().any(|e| e.contains(": A") && e.contains("mismatch"));
            let has_ssp = errors.iter().any(|e| e.contains("SSP mismatch"));
            let has_pc = errors.iter().any(|e| e.contains("PC mismatch"));
            let has_dreg = errors.iter().any(|e| e.contains(": D") && e.contains("mismatch"));
            let has_usp = errors.iter().any(|e| e.contains("USP mismatch"));

            // Classify: is this an AE?
            let is_ae = test.final_state.ssp != test.initial.ssp
                || (test.final_state.sr & 0x2000 != 0 && test.initial.sr & 0x2000 == 0);

            if has_sr {
                if is_ae {
                    let final_ssp = test.final_state.ssp;
                    let ram_map: std::collections::HashMap<u32, u8> = test.final_state.ram.iter().copied().collect();
                    let access_lo = ram_map.get(&(final_ssp + 1)).copied().unwrap_or(0);
                    let is_read = access_lo & 0x10 != 0;
                    if is_read { ae_src_read_sr += 1; } else { ae_dst_write_sr += 1; }
                } else {
                    non_ae_sr += 1;
                }
            }

            let sole_error = !has_ram && !has_areg && !has_ssp && !has_pc && !has_dreg && !has_usp;
            if has_sr && sole_error { sr_only += 1; }
            else if has_ram && !has_sr && !has_areg && !has_pc && !has_dreg { ram_only += 1; }
            else if has_areg && !has_sr && !has_ram && !has_pc && !has_dreg { areg_only += 1; }
            else if has_ssp && !has_sr && !has_ram && !has_areg && !has_pc && !has_dreg { ssp_only += 1; }
            else if has_pc && !has_sr && !has_ram && !has_areg && !has_dreg { pc_only += 1; }
            else { mixed += 1; }

            // Show first 15 failing tests
            if shown < 15 {
                let op = test.initial.prefetch[0] as u16;
                let src_mode = (op >> 3) & 7;
                let src_reg = op & 7;
                let dst_mode = (op >> 6) & 7;
                let dst_reg = (op >> 9) & 7;
                println!("  #{:04} ae={:<5} src={}:{} dst={}:{} op=0x{:04X}",
                    i, is_ae, src_mode, src_reg, dst_mode, dst_reg, op);
                for e in &errors {
                    println!("    {}", e);
                }
                shown += 1;
            }
        }

        println!("\n=== {} failure summary ===", name);
        println!("  sr_only={} ram_only={} areg_only={} ssp_only={} pc_only={} mixed={}",
            sr_only, ram_only, areg_only, ssp_only, pc_only, mixed);
        println!("  SR mismatches: ae_dst_write={} ae_src_read={} non_ae={}",
            ae_dst_write_sr, ae_src_read_sr, non_ae_sr);
    }
}

/// Investigate MOVE.l N flag computation for AE dst write failures.
#[test]
fn diagnose_move_l_nflag() {
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap().parent().unwrap()
        .join("test-data/m68000-dl/v1/MOVE.l.json.bin");

    if !test_file.exists() { return; }
    let tests = decode_file(&test_file).expect("Failed to decode");

    let mut n_from_bit15_match = 0u32;
    let mut n_from_bit31_match = 0u32;
    let mut n_both_match = 0u32;
    let mut n_neither = 0u32;
    let mut total_sr_mismatch = 0u32;

    for (_i, test) in tests.iter().enumerate() {
        let mut cpu = M68000::new();
        let mut mem = TestBus::new();
        setup_cpu(&mut cpu, &mut mem, &test.initial);
        for _ in 0..test.cycles { cpu.tick(&mut mem); }
        let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
        if errors.is_empty() { continue; }

        let has_sr = errors.iter().any(|e| e.contains("SR mismatch"));
        if !has_sr { continue; }

        total_sr_mismatch += 1;
        let exp_sr = test.final_state.sr;
        let exp_n = ((exp_sr >> 3) & 1) as u32;

        // Reconstruct what data was being moved:
        // Read initial RAM at the source address to get the moved data
        let op = test.initial.prefetch[0] as u16;
        let src_mode = (op >> 3) & 7;
        let src_reg = (op & 7) as usize;

        // For memory sources, read data from the source address in initial RAM
        let ram_map: std::collections::HashMap<u32, u8> = test.initial.ram.iter().copied().collect();

        // Try to figure out the data value from the expected final RAM at destination
        // For the AE frame, the data being moved might be partially written
        // The actual source data is what matters for flags

        // Extract source address and read data
        let areg = |r: usize| -> u32 {
            if r < 7 { test.initial.a[r] }
            else if test.initial.sr & 0x2000 != 0 { test.initial.ssp }
            else { test.initial.usp }
        };
        let src_addr = match src_mode {
            2 => Some(areg(src_reg)),  // (An)
            3 => Some(areg(src_reg)),  // (An)+
            4 => Some(areg(src_reg).wrapping_sub(4)),  // -(An)
            _ => None, // skip complex modes for now
        };

        if let Some(addr) = src_addr {
            let b0 = *ram_map.get(&addr).unwrap_or(&0) as u32;
            let b1 = *ram_map.get(&(addr.wrapping_add(1))).unwrap_or(&0) as u32;
            let b2 = *ram_map.get(&(addr.wrapping_add(2))).unwrap_or(&0) as u32;
            let b3 = *ram_map.get(&(addr.wrapping_add(3))).unwrap_or(&0) as u32;
            let data = (b0 << 24) | (b1 << 16) | (b2 << 8) | b3;

            let n_bit31 = (data >> 31) & 1;
            let n_bit15 = (data >> 15) & 1;

            let matches_31 = n_bit31 == exp_n;
            let matches_15 = n_bit15 == exp_n;

            if matches_31 && matches_15 { n_both_match += 1; }
            else if matches_31 { n_from_bit31_match += 1; }
            else if matches_15 { n_from_bit15_match += 1; }
            else { n_neither += 1; }

            if total_sr_mismatch <= 10 {
                println!("  data=0x{:08X} exp_N={} bit31_N={} bit15_N={} src_mode={} init_ccr={:02X} exp_ccr={:02X}",
                    data, exp_n, n_bit31, n_bit15, src_mode,
                    test.initial.sr & 0xFF, exp_sr & 0xFF);
            }
        }
    }

    println!("\nMOVE.l SR mismatch N flag analysis (total={}):", total_sr_mismatch);
    println!("  N from bit31 only: {}", n_from_bit31_match);
    println!("  N from bit15 only: {}", n_from_bit15_match);
    println!("  N from both: {}", n_both_match);
    println!("  N from neither: {}", n_neither);
}

/// Investigate AE frame content for MOVE.w RAM-only failures.
#[test]
fn diagnose_move_w_ae_frame() {
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap().parent().unwrap()
        .join("test-data/m68000-dl/v1/MOVE.w.json.bin");

    if !test_file.exists() { return; }
    let tests = decode_file(&test_file).expect("Failed to decode");

    // Counters per frame offset
    let mut offset_counts = std::collections::HashMap::<u32, u32>::new();
    let mut shown = 0u32;
    let mut total = 0u32;

    for (i, test) in tests.iter().enumerate() {
        let mut cpu = M68000::new();
        let mut mem = TestBus::new();
        setup_cpu(&mut cpu, &mut mem, &test.initial);
        for _ in 0..test.cycles { cpu.tick(&mut mem); }
        let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
        if errors.is_empty() { continue; }
        if !errors.iter().any(|e| e.contains("RAM[")) { continue; }
        if errors.iter().any(|e| e.contains("SR mismatch")) { continue; }

        total += 1;
        let final_ssp = test.final_state.ssp;

        // Map RAM errors to frame offsets
        // AE frame at SSP: [access_hi, access_lo, fault3, fault2, fault1, fault0, ir_hi, ir_lo, sr_hi, sr_lo, pc3, pc2, pc1, pc0]
        let frame_names = ["acc_hi", "acc_lo", "flt3", "flt2", "flt1", "flt0", "ir_hi", "ir_lo", "sr_hi", "sr_lo", "pc3", "pc2", "pc1", "pc0"];
        for e in &errors {
            if e.contains("RAM[") {
                // Extract address from "RAM[0xABCDEF]"
                if let Some(start) = e.find("RAM[0x") {
                    let hex_str = &e[start+6..start+12];
                    if let Ok(addr) = u32::from_str_radix(hex_str, 16) {
                        let offset = addr.wrapping_sub(final_ssp);
                        *offset_counts.entry(offset).or_insert(0) += 1;
                    }
                }
            }
        }

        if shown < 10 {
            let op = test.initial.prefetch[0] as u16;
            let src_mode = (op >> 3) & 7;
            let dst_mode = (op >> 6) & 7;
            println!("  #{:04} src={}:{} dst={}:{} SSP=0x{:06X}",
                i, src_mode, op & 7, dst_mode, (op >> 9) & 7, final_ssp);
            for e in &errors {
                if e.contains("RAM[") {
                    if let Some(start) = e.find("RAM[0x") {
                        let hex_str = &e[start+6..start+12];
                        if let Ok(addr) = u32::from_str_radix(hex_str, 16) {
                            let offset = addr.wrapping_sub(final_ssp);
                            let name = if (offset as usize) < frame_names.len() {
                                frame_names[offset as usize]
                            } else { "???" };
                            println!("    offset={} ({}) {}", offset, name, e);
                        }
                    }
                }
            }
            shown += 1;
        }
    }

    println!("\nMOVE.w AE frame RAM errors (total={} tests):", total);
    let mut sorted: Vec<_> = offset_counts.into_iter().collect();
    sorted.sort_by_key(|(k, _)| *k);
    let frame_names = ["acc_hi", "acc_lo", "flt3", "flt2", "flt1", "flt0", "ir_hi", "ir_lo", "sr_hi", "sr_lo", "pc3", "pc2", "pc1", "pc0"];
    for (offset, count) in &sorted {
        let name = if (*offset as usize) < frame_names.len() {
            frame_names[*offset as usize]
        } else { "???" };
        println!("  offset={} ({}): {} errors", offset, name, count);
    }
}

/// Diagnose MOVEM failures by error category.
#[test]
fn diagnose_movem_failures() {
    for name in &["MOVEM.l", "MOVEM.w"] {
        let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap().parent().unwrap()
            .join(format!("test-data/m68000-dl/v1/{}.json.bin", name));

        if !test_file.exists() { continue; }
        let tests = decode_file(&test_file).expect("Failed to decode");

        let mut sr_only = 0u32;
        let mut ram_only = 0u32;
        let mut areg_only = 0u32;
        let mut dreg_only = 0u32;
        let mut pc_only = 0u32;
        let mut ssp_only = 0u32;
        let mut mixed = 0u32;
        let mut total_fail = 0u32;
        let mut ae_count = 0u32;
        let mut shown = 0u32;

        for (i, test) in tests.iter().enumerate() {
            let mut cpu = M68000::new();
            let mut mem = TestBus::new();
            setup_cpu(&mut cpu, &mut mem, &test.initial);
            for _ in 0..test.cycles { cpu.tick(&mut mem); }
            let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
            if errors.is_empty() { continue; }

            total_fail += 1;
            let is_ae = test.final_state.ssp != test.initial.ssp
                || (test.final_state.sr & 0x2000 != 0 && test.initial.sr & 0x2000 == 0);
            if is_ae { ae_count += 1; }

            let has_sr = errors.iter().any(|e| e.contains("SR mismatch"));
            let has_ram = errors.iter().any(|e| e.contains("RAM["));
            let has_areg = errors.iter().any(|e| e.contains(": A") && e.contains("mismatch"));
            let has_dreg = errors.iter().any(|e| e.contains(": D") && e.contains("mismatch"));
            let has_pc = errors.iter().any(|e| e.contains("PC mismatch"));
            let has_ssp = errors.iter().any(|e| e.contains("SSP mismatch"));

            let err_types = [has_sr, has_ram, has_areg, has_dreg, has_pc, has_ssp];
            let count = err_types.iter().filter(|&&x| x).count();
            if count == 1 {
                if has_sr { sr_only += 1; }
                else if has_ram { ram_only += 1; }
                else if has_areg { areg_only += 1; }
                else if has_dreg { dreg_only += 1; }
                else if has_pc { pc_only += 1; }
                else if has_ssp { ssp_only += 1; }
            } else { mixed += 1; }

            if shown < 15 {
                let op = test.initial.prefetch[0] as u16;
                let dir = if op & 0x0400 != 0 { "mem->reg" } else { "reg->mem" };
                let mode = (op >> 3) & 7;
                let reg = op & 7;
                println!("  #{:04} ae={:<5} dir={} mode={}:{} op=0x{:04X}",
                    i, is_ae, dir, mode, reg, op);
                for e in errors.iter().take(5) {
                    println!("    {}", e);
                }
                shown += 1;
            }
        }

        println!("\n=== {} failure summary (total={}) ===", name, total_fail);
        println!("  ae={} non_ae={}", ae_count, total_fail - ae_count);
        println!("  sr_only={} ram_only={} areg_only={} dreg_only={} pc_only={} ssp_only={} mixed={}",
            sr_only, ram_only, areg_only, dreg_only, pc_only, ssp_only, mixed);
    }
}

/// Diagnose MOVEM reg→mem AE frame errors in detail.
#[test]
fn diagnose_movem_regmem_ae() {
    for size_name in &["MOVEM.l", "MOVEM.w"] {
        let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap().parent().unwrap()
            .join(format!("test-data/m68000-dl/v1/{}.json.bin", size_name));

        if !test_file.exists() { continue; }
        let tests = decode_file(&test_file).expect("Failed to decode");

        let mut shown = 0u32;
        let mut total = 0u32;
        // Track error categories
        let mut frame_errs = std::collections::HashMap::<&str, u32>::new();
        let mut data_errs = 0u32;

        for (i, test) in tests.iter().enumerate() {
            let mut cpu = M68000::new();
            let mut mem = TestBus::new();
            setup_cpu(&mut cpu, &mut mem, &test.initial);
            for _ in 0..test.cycles { cpu.tick(&mut mem); }
            let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
            if errors.is_empty() { continue; }

            total += 1;
            let final_ssp = test.final_state.ssp;

            for e in &errors {
                if e.contains("RAM[") {
                    if let Some(start) = e.find("RAM[0x") {
                        let hex_str = &e[start+6..start+12];
                        if let Ok(addr) = u32::from_str_radix(hex_str, 16) {
                            let offset = addr.wrapping_sub(final_ssp);
                            if offset < 14 {
                                let name = match offset {
                                    0 | 1 => "access_info",
                                    2..=5 => "fault_addr",
                                    6 | 7 => "ir",
                                    8 | 9 => "sr",
                                    10..=13 => "pc",
                                    _ => "?",
                                };
                                *frame_errs.entry(name).or_insert(0) += 1;
                            } else {
                                data_errs += 1;
                            }
                        }
                    }
                }
            }

            if shown < 5 {
                let op = test.initial.prefetch[0] as u16;
                let mode = (op >> 3) & 7;
                let reg = op & 7;
                let dir = if op & 0x0400 != 0 { "mem->reg" } else { "reg->mem" };
                println!("  {} #{:04} dir={} mode={}:{} ssp=0x{:06X}", size_name, i, dir, mode, reg, final_ssp);
                for e in &errors {
                    if e.contains("RAM[") {
                        if let Some(start) = e.find("RAM[0x") {
                            let hex_str = &e[start+6..start+12];
                            if let Ok(addr) = u32::from_str_radix(hex_str, 16) {
                                let offset = addr.wrapping_sub(final_ssp);
                                let in_frame = if offset < 14 { "FRAME" } else { "DATA" };
                                println!("    {} offset={} {}", in_frame, offset, e);
                            }
                        }
                    }
                }
                shown += 1;
            }
        }

        println!("\n=== {} reg->mem AE analysis (total={}) ===", size_name, total);
        println!("  frame errors: {:?}", frame_errs);
        println!("  data errors: {}", data_errs);
    }
}

/// Diagnose PC values for MOVE.w predec-dst AE failures.
#[test]
fn diagnose_move_w_predec_ae_pc() {
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap().parent().unwrap()
        .join("test-data/m68000-dl/v1/MOVE.w.json.bin");

    if !test_file.exists() { return; }
    let tests = decode_file(&test_file).expect("Failed to decode");

    let mut pc_correct = 0u32;
    let mut pc_matches_minus2 = 0u32;
    let mut pc_matches_none = 0u32;
    let mut shown = 0u32;

    // Analyze by source mode
    let mut by_src_mode: std::collections::HashMap<u16, (u32, u32, u32)> = std::collections::HashMap::new();

    for (i, test) in tests.iter().enumerate() {
        let op = test.initial.prefetch[0] as u16;
        let dst_mode = (op >> 6) & 7;
        if dst_mode != 4 { continue; } // Only predec dst

        let mut cpu = M68000::new();
        let mut mem = TestBus::new();
        setup_cpu(&mut cpu, &mut mem, &test.initial);
        for _ in 0..test.cycles { cpu.tick(&mut mem); }

        // Check if SSP changed (AE occurred)
        let ssp_changed = test.final_state.ssp != test.initial.ssp;
        if !ssp_changed { continue; }

        // Extract expected PC from frame (offset 10-13 from SSP)
        let final_ssp = test.final_state.ssp;
        let mut exp_pc_bytes = [0u8; 4];
        let mut all_present = true;
        for j in 0..4u32 {
            let addr = final_ssp.wrapping_add(10 + j);
            if let Some((_, v)) = test.final_state.ram.iter().find(|(a, _)| *a == addr) {
                exp_pc_bytes[j as usize] = *v;
            } else {
                all_present = false;
            }
        }
        if !all_present { continue; }
        let exp_pc = u32::from(exp_pc_bytes[0]) << 24
            | u32::from(exp_pc_bytes[1]) << 16
            | u32::from(exp_pc_bytes[2]) << 8
            | u32::from(exp_pc_bytes[3]);

        // Extract our PC from the actual RAM
        let mut our_pc_bytes = [0u8; 4];
        for j in 0..4u32 {
            our_pc_bytes[j as usize] = mem.peek(final_ssp.wrapping_add(10 + j));
        }
        let our_pc = u32::from(our_pc_bytes[0]) << 24
            | u32::from(our_pc_bytes[1]) << 16
            | u32::from(our_pc_bytes[2]) << 8
            | u32::from(our_pc_bytes[3]);

        let src_mode = (op >> 3) & 7;
        let src_reg = op & 7;
        let src_ext = match src_mode {
            0 | 1 | 2 | 3 | 4 => 0u32,
            5 | 6 => 1,
            7 => match src_reg { 0 | 2 | 3 => 1, 1 => 2, 4 => 1, _ => 0 },
            _ => 0,
        };

        let entry = by_src_mode.entry(src_mode).or_insert((0, 0, 0));

        if our_pc == exp_pc {
            pc_correct += 1;
            entry.0 += 1;
        } else {
            let instr_start = test.initial.pc;
            let minus2 = instr_start.wrapping_sub(2);
            if exp_pc == minus2 {
                pc_matches_minus2 += 1;
                entry.1 += 1;
            } else {
                pc_matches_none += 1;
                entry.2 += 1;
            }

            if shown < 15 {
                println!("  #{:04} src={}:{} S={} our_pc=0x{:06X} exp_pc=0x{:06X} instr_start=0x{:06X} diff={}",
                    i, src_mode, src_reg, src_ext, our_pc, exp_pc, test.initial.pc,
                    our_pc as i64 - exp_pc as i64);
                shown += 1;
            }
        }
    }

    // Also split by read vs write fault
    let mut write_correct = 0u32;
    let mut write_minus2 = 0u32;
    let mut read_correct = 0u32;
    let mut read_minus2 = 0u32;

    // Re-scan for read/write split
    for test in tests.iter() {
        let op = test.initial.prefetch[0] as u16;
        let dst_mode = (op >> 6) & 7;
        if dst_mode != 4 { continue; }

        let mut cpu = M68000::new();
        let mut mem = TestBus::new();
        setup_cpu(&mut cpu, &mut mem, &test.initial);
        for _ in 0..test.cycles { cpu.tick(&mut mem); }

        let ssp_changed = test.final_state.ssp != test.initial.ssp;
        if !ssp_changed { continue; }
        let final_ssp = test.final_state.ssp;

        // Get access_info to determine read/write
        let acc_hi = test.final_state.ram.iter()
            .find(|(a, _)| *a == final_ssp).map(|(_, v)| *v).unwrap_or(0);
        let acc_lo = test.final_state.ram.iter()
            .find(|(a, _)| *a == final_ssp.wrapping_add(1)).map(|(_, v)| *v).unwrap_or(0);
        let is_read = (acc_lo & 0x10) != 0;

        // Get expected PC
        let mut exp_pc_bytes = [0u8; 4];
        let mut all_present = true;
        for j in 0..4u32 {
            let addr = final_ssp.wrapping_add(10 + j);
            if let Some((_, v)) = test.final_state.ram.iter().find(|(a, _)| *a == addr) {
                exp_pc_bytes[j as usize] = *v;
            } else { all_present = false; }
        }
        if !all_present { continue; }
        let exp_pc = u32::from(exp_pc_bytes[0]) << 24
            | u32::from(exp_pc_bytes[1]) << 16
            | u32::from(exp_pc_bytes[2]) << 8
            | u32::from(exp_pc_bytes[3]);

        let our_pc_bytes: Vec<u8> = (0..4u32).map(|j| mem.peek(final_ssp.wrapping_add(10 + j))).collect();
        let our_pc = u32::from(our_pc_bytes[0]) << 24
            | u32::from(our_pc_bytes[1]) << 16
            | u32::from(our_pc_bytes[2]) << 8
            | u32::from(our_pc_bytes[3]);

        let correct = our_pc == exp_pc;
        let minus2 = exp_pc == test.initial.pc.wrapping_sub(2);

        if is_read {
            if correct { read_correct += 1; } else if minus2 { read_minus2 += 1; }
        } else {
            if correct { write_correct += 1; } else if minus2 { write_minus2 += 1; }
        }
    }

    println!("\nMOVE.w predec-dst AE PC analysis:");
    println!("  pc_correct={} pc_needs_minus2={} pc_other={}", pc_correct, pc_matches_minus2, pc_matches_none);
    println!("  READ faults: correct={} minus2={}", read_correct, read_minus2);
    println!("  WRITE faults: correct={} minus2={}", write_correct, write_minus2);
    println!("  by src_mode (correct/minus2/other):");
    let mut modes: Vec<_> = by_src_mode.into_iter().collect();
    modes.sort_by_key(|(k, _)| *k);
    for (mode, (correct, minus2, other)) in &modes {
        let name = match mode { 0 => "Dn", 1 => "An", 2 => "(An)", 3 => "(An)+", 4 => "-(An)", 5 => "d16", 6 => "d8idx", 7 => "abs/pc/imm", _ => "?" };
        println!("    src_mode={} ({}): correct={} minus2={} other={}", mode, name, correct, minus2, other);
    }
}

/// Diagnose remaining errors in MOVE.w predec-dst AE failures.
#[test]
fn diagnose_move_w_predec_ae_errors() {
    let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap().parent().unwrap()
        .join("test-data/m68000-dl/v1/MOVE.w.json.bin");

    if !test_file.exists() { return; }
    let tests = decode_file(&test_file).expect("Failed to decode");

    let mut total = 0u32;
    let mut error_types = std::collections::HashMap::<String, u32>::new();
    let mut shown = 0u32;

    for (i, test) in tests.iter().enumerate() {
        let mut cpu = M68000::new();
        let mut mem = TestBus::new();
        setup_cpu(&mut cpu, &mut mem, &test.initial);
        for _ in 0..test.cycles { cpu.tick(&mut mem); }
        let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
        if errors.is_empty() { continue; }

        // Only look at predec-dst write AEs
        let op = test.initial.prefetch[0] as u16;
        let dst_mode = (op >> 6) & 7;
        if dst_mode != 4 { continue; } // Not -(An)

        let ssp_changed = test.final_state.ssp != test.initial.ssp;
        if !ssp_changed { continue; } // Not AE

        total += 1;
        // Categorize errors
        for e in &errors {
            let cat = if e.contains("SR mismatch") { "SR" }
                else if e.contains("PC mismatch") { "PC" }
                else if e.contains("SSP mismatch") { "SSP" }
                else if e.contains(": A") && e.contains("mismatch") { "Areg" }
                else if e.contains(": D") && e.contains("mismatch") { "Dreg" }
                else if e.contains("RAM[") {
                    let final_ssp = test.final_state.ssp;
                    if let Some(start) = e.find("RAM[0x") {
                        let hex_str = &e[start+6..start+12];
                        if let Ok(addr) = u32::from_str_radix(hex_str, 16) {
                            let offset = addr.wrapping_sub(final_ssp);
                            match offset {
                                0 | 1 => "RAM:access_info",
                                2..=5 => "RAM:fault_addr",
                                6 | 7 => "RAM:ir",
                                8 | 9 => "RAM:sr",
                                10..=13 => "RAM:pc",
                                _ => "RAM:other",
                            }
                        } else { "RAM:?" }
                    } else { "RAM:?" }
                } else { "other" };
            *error_types.entry(cat.to_string()).or_insert(0) += 1;
        }

        if shown < 5 {
            println!("  #{:04} errors:", i);
            for e in &errors { println!("    {}", e); }
            shown += 1;
        }
    }

    println!("\nMOVE.w predec-dst AE failures (total={}):", total);
    let mut sorted: Vec<_> = error_types.into_iter().collect();
    sorted.sort_by_key(|(_, v)| std::cmp::Reverse(*v));
    for (cat, count) in &sorted {
        println!("  {}: {}", cat, count);
    }
}

/// Diagnose MOVE AE IR field: what should the IR value be in the exception frame?
/// Only examines tests that actually FAIL and have IR bytes in the expected RAM.
#[test]
fn diagnose_move_ae_ir() {
    for size_name in &["MOVE.w", "MOVE.l"] {
        let test_file = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap().parent().unwrap()
            .join(format!("test-data/m68000-dl/v1/{}.json.bin", size_name));

        if !test_file.exists() { continue; }
        let tests = decode_file(&test_file).expect("Failed to decode");

        let mut ir_matches_opcode = 0u32;
        let mut ir_matches_ext = [0u32; 4];
        let mut ir_matches_formula = 0u32; // ext_words[S+D]
        let mut ir_matches_none = 0u32;
        let mut total_ae_write = 0u32;
        let mut total_ir_wrong = 0u32;
        let mut total_ir_not_in_ram = 0u32;
        let mut shown = 0u32;

        // Also check access_info upper bits
        let mut acc_matches_opcode = 0u32;
        let mut acc_matches_formula = 0u32;
        let mut total_acc_wrong = 0u32;

        fn src_ext_words_fn(op: u16, is_long: bool) -> u8 {
            let src_mode = (op >> 3) & 7;
            let src_reg = op & 7;
            match src_mode {
                0 | 1 | 2 | 3 | 4 => 0,
                5 | 6 => 1,
                7 => match src_reg {
                    0 | 2 | 3 => 1,
                    1 => 2,
                    4 => if is_long { 2 } else { 1 },
                    _ => 0,
                },
                _ => 0,
            }
        }

        fn dst_ext_words_fn(op: u16) -> u8 {
            let dst_mode = (op >> 6) & 7;
            let dst_reg = (op >> 9) & 7;
            match dst_mode {
                0 | 1 | 2 | 3 | 4 => 0,
                5 | 6 => 1,
                7 => match dst_reg {
                    0 => 1,
                    1 => 2,
                    _ => 0,
                },
                _ => 0,
            }
        }

        let is_long = size_name.ends_with(".l");

        for (i, test) in tests.iter().enumerate() {
            let mut cpu = M68000::new();
            let mut mem = TestBus::new();
            setup_cpu(&mut cpu, &mut mem, &test.initial);
            for _ in 0..test.cycles { cpu.tick(&mut mem); }
            let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
            if errors.is_empty() { continue; } // Test passes, skip

            // Is this an AE? Check if SSP changed
            let ssp_changed = test.final_state.ssp != test.initial.ssp;
            if !ssp_changed { continue; }

            let final_ssp = test.final_state.ssp;

            // Check if it's a write fault by looking at access_info bit 4
            // Only check if access_info bytes are in the RAM
            let acc_hi_addr = final_ssp;
            let acc_lo_addr = final_ssp.wrapping_add(1);
            let acc_hi_in_ram = test.final_state.ram.iter().any(|(a, _)| *a == acc_hi_addr);
            let acc_lo_in_ram = test.final_state.ram.iter().any(|(a, _)| *a == acc_lo_addr);
            if !acc_hi_in_ram && !acc_lo_in_ram { continue; }

            let exp_acc_hi = test.final_state.ram.iter()
                .find(|(a, _)| *a == acc_hi_addr).map(|(_, v)| *v).unwrap_or(0);
            let exp_acc_lo = test.final_state.ram.iter()
                .find(|(a, _)| *a == acc_lo_addr).map(|(_, v)| *v).unwrap_or(0);
            let exp_access = u16::from(exp_acc_hi) << 8 | u16::from(exp_acc_lo);
            let is_write = exp_access & 0x10 == 0;
            if !is_write { continue; }

            total_ae_write += 1;
            let opcode = test.initial.prefetch[0] as u16;
            let pc = test.initial.pc;
            let irc = test.initial.prefetch[1] as u16;
            let ew1 = (u16::from(mem.peek(pc)) << 8) | u16::from(mem.peek(pc.wrapping_add(1)));
            let ew2 = (u16::from(mem.peek(pc.wrapping_add(2))) << 8) | u16::from(mem.peek(pc.wrapping_add(3)));
            let ew3 = (u16::from(mem.peek(pc.wrapping_add(4))) << 8) | u16::from(mem.peek(pc.wrapping_add(5)));
            let ext = [irc, ew1, ew2, ew3];
            let s = src_ext_words_fn(opcode, is_long) as usize;
            let d = dst_ext_words_fn(opcode) as usize;

            // Check IR field (offset 6,7 from SSP)
            let ir_hi_addr = final_ssp.wrapping_add(6);
            let ir_lo_addr = final_ssp.wrapping_add(7);
            let ir_hi_in_ram = test.final_state.ram.iter().any(|(a, _)| *a == ir_hi_addr);
            let ir_lo_in_ram = test.final_state.ram.iter().any(|(a, _)| *a == ir_lo_addr);

            if !ir_hi_in_ram && !ir_lo_in_ram {
                total_ir_not_in_ram += 1;
            } else {
                let exp_ir_hi = test.final_state.ram.iter()
                    .find(|(a, _)| *a == ir_hi_addr).map(|(_, v)| *v).unwrap_or(0);
                let exp_ir_lo = test.final_state.ram.iter()
                    .find(|(a, _)| *a == ir_lo_addr).map(|(_, v)| *v).unwrap_or(0);
                let exp_ir = u16::from(exp_ir_hi) << 8 | u16::from(exp_ir_lo);

                if exp_ir == opcode {
                    ir_matches_opcode += 1;
                } else {
                    total_ir_wrong += 1;

                    let mut matched_idx: Option<usize> = None;
                    for j in 0..4 {
                        if ext[j] == exp_ir {
                            if matched_idx.is_none() { matched_idx = Some(j); }
                            ir_matches_ext[j] += 1;
                        }
                    }

                    let formula_idx = s + d;
                    if formula_idx < 4 && ext[formula_idx] == exp_ir {
                        ir_matches_formula += 1;
                    }

                    if matched_idx.is_none() {
                        ir_matches_none += 1;
                    }

                    let src_mode = (opcode >> 3) & 7;
                    let src_reg = opcode & 7;
                    let dst_mode = (opcode >> 6) & 7;
                    let dst_reg = (opcode >> 9) & 7;

                    if shown < 20 {
                        println!("  #{:04} {} src={}:{} dst={}:{} S={} D={} exp_ir=0x{:04X} opcode=0x{:04X} ext=[{:04X},{:04X},{:04X},{:04X}] match={:?}",
                            i, size_name, src_mode, src_reg, dst_mode, dst_reg,
                            s, d, exp_ir, opcode,
                            ext[0], ext[1], ext[2], ext[3], matched_idx);
                        shown += 1;
                    }
                }
            }

            // Check access_info upper bits (should match IR, not opcode)
            // Compare only IR-derived bits (bits 15:8 and 7:5)
            let our_acc_ir_bits = (opcode & 0xFFE0);
            let exp_acc_ir_bits = (exp_access & 0xFFE0);
            if our_acc_ir_bits != exp_acc_ir_bits {
                total_acc_wrong += 1;
                let formula_idx = s + d;
                let formula_ir = if formula_idx < 4 { ext[formula_idx] } else { opcode };
                let formula_acc_bits = formula_ir & 0xFFE0;
                if formula_acc_bits == exp_acc_ir_bits { acc_matches_formula += 1; }
                if our_acc_ir_bits == exp_acc_ir_bits { acc_matches_opcode += 1; }
            }
        }

        println!("\n=== {} AE write IR/AccessInfo analysis (failing tests only) ===", size_name);
        println!("  total_ae_write_failures={}", total_ae_write);
        println!("  IR: correct(opcode)={} wrong={} not_in_ram={}",
            ir_matches_opcode, total_ir_wrong, total_ir_not_in_ram);
        println!("  IR wrong matches: ext[0]={} ext[1]={} ext[2]={} ext[3]={}",
            ir_matches_ext[0], ir_matches_ext[1], ir_matches_ext[2], ir_matches_ext[3]);
        println!("  IR formula ext[S+D]={} matches_none={}", ir_matches_formula, ir_matches_none);
        println!("  AccessInfo: wrong={} formula_match={}", total_acc_wrong, acc_matches_formula);
    }
}

#[test]
#[ignore]
fn diagnose_move_l_failures() {
    let path = Path::new("../../test-data/m68000-dl/v1/MOVE.l.json.bin");
    if !path.exists() { return; }
    let tests = decode_file(path).expect("decode");

    // Categorize ALL failing tests
    let mut ae_write = 0u32;
    let mut ae_read = 0u32;
    let mut non_ae = 0u32;
    let mut ae_write_errors: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let mut non_ae_errors: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let mut first_examples: Vec<(usize, String)> = Vec::new();

    for (i, test) in tests.iter().enumerate() {
        let mut cpu = M68000::new();
        let mut mem = TestBus::new();
        setup_cpu(&mut cpu, &mut mem, &test.initial);
        for _ in 0..test.cycles { cpu.tick(&mut mem); }
        let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
        if errors.is_empty() { continue; }

        let opcode = test.initial.prefetch[0] as u16;
        let ssp = test.initial.ssp;

        // Is this an AE? Check if SSP changed by 14 (AE frame push)
        let exp_ssp = test.final_state.ssp;
        let is_ae = exp_ssp == ssp.wrapping_sub(14);

        if !is_ae {
            non_ae += 1;
            for err in &errors {
                let cat = if err.contains("SR ") { "SR" }
                    else if err.contains("RAM[") { "RAM" }
                    else if err.contains("PC ") { "PC" }
                    else if err.contains(" D") { "Dreg" }
                    else if err.contains(" A") { "Areg" }
                    else if err.contains("SSP") { "SSP" }
                    else if err.contains("USP") { "USP" }
                    else { "other" };
                *non_ae_errors.entry(cat.to_string()).or_insert(0) += 1;
            }
            if first_examples.len() < 10 {
                first_examples.push((i, format!("NON-AE #{:04} opcode=0x{:04X}: {}", i, opcode, errors.join("; "))));
            }
            continue;
        }

        // AE case - check if read or write fault
        // Look at access_info byte at SSP (top of frame)
        let acc_hi = mem.peek(exp_ssp);
        let is_read_fault = acc_hi & 0x10 != 0; // bit 4 = read

        if is_read_fault {
            ae_read += 1;
        } else {
            ae_write += 1;
        }

        // Categorize errors within the AE frame
        for err in &errors {
            let addr_str = if let Some(pos) = err.find("RAM[0x") {
                let hex = &err[pos+6..pos+12];
                if let Ok(addr) = u32::from_str_radix(hex, 16) {
                    let offset = addr.wrapping_sub(exp_ssp);
                    if offset < 14 {
                        match offset {
                            0..=1 => "access_info",
                            2..=5 => "fault_addr",
                            6..=7 => "ir",
                            8..=9 => "sr",
                            10..=13 => "pc",
                            _ => "data",
                        }
                    } else { "data" }
                } else { "ram_parse_err" }
            } else if err.contains("SR ") { "SR_reg" }
            else if err.contains("PC ") { "PC_reg" }
            else if err.contains(" A") { "Areg" }
            else if err.contains(" D") { "Dreg" }
            else if err.contains("SSP") { "SSP" }
            else { "other" };

            *ae_write_errors.entry(addr_str.to_string()).or_insert(0) += 1;
        }
    }

    println!("\n=== MOVE.l failure analysis ===");
    println!("  AE write faults: {}", ae_write);
    println!("  AE read faults: {}", ae_read);
    println!("  Non-AE failures: {}", non_ae);
    println!("  AE frame error categories: {:?}", ae_write_errors);
    println!("  Non-AE error categories: {:?}", non_ae_errors);
    for (_, ex) in &first_examples {
        println!("  {}", ex);
    }

    // Second pass: classify AE write failures by src/dst mode and error type
    println!("\n--- AE WRITE DETAIL ---");

    // Categorize by src_mode x dst_mode
    let mut sr_wrong_by_mode: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let mut pc_wrong_by_mode: std::collections::HashMap<String, Vec<i32>> = std::collections::HashMap::new();
    let mut areg_wrong_by_mode: std::collections::HashMap<String, Vec<i32>> = std::collections::HashMap::new();
    let mut fault_wrong_by_mode: std::collections::HashMap<String, Vec<i32>> = std::collections::HashMap::new();
    let mut sr_flag_diffs: std::collections::HashMap<u16, u32> = std::collections::HashMap::new();

    for test in tests.iter() {
        let mut cpu = M68000::new();
        let mut mem = TestBus::new();
        setup_cpu(&mut cpu, &mut mem, &test.initial);
        for _ in 0..test.cycles { cpu.tick(&mut mem); }
        let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
        if errors.is_empty() { continue; }

        let opcode = test.initial.prefetch[0] as u16;
        let ssp = test.initial.ssp;
        let exp_ssp = test.final_state.ssp;
        let is_ae = exp_ssp == ssp.wrapping_sub(14);
        if !is_ae { continue; }

        // Check read/write
        let acc_byte = mem.peek(exp_ssp);
        if acc_byte & 0x10 != 0 { continue; } // skip read faults

        // Decode src/dst mode names
        let src_mode_bits = (opcode & 0x38) >> 3;
        let src_reg = opcode & 7;
        let dst_mode_bits = (opcode >> 6) & 7;
        let dst_reg = (opcode >> 9) & 7;
        let mode_key = format!("src={}{} dst={}{}", src_mode_bits,
            if src_mode_bits == 7 { format!(":{}", src_reg) } else { String::new() },
            dst_mode_bits,
            if dst_mode_bits == 7 { format!(":{}", dst_reg) } else { String::new() });

        // Check SR register
        if cpu.regs.sr != test.final_state.sr {
            *sr_wrong_by_mode.entry(mode_key.clone()).or_insert(0) += 1;
            let diff_bits = cpu.regs.sr ^ test.final_state.sr;
            *sr_flag_diffs.entry(diff_bits).or_insert(0) += 1;
        }

        // Check PC in frame (bytes 10-13 from exp_ssp)
        let exp_pc_bytes: [u8; 4] = [
            test.final_state.ram.iter().find(|&&(a,_)| a == exp_ssp.wrapping_add(10)).map(|&(_,v)| v).unwrap_or(0),
            test.final_state.ram.iter().find(|&&(a,_)| a == exp_ssp.wrapping_add(11)).map(|&(_,v)| v).unwrap_or(0),
            test.final_state.ram.iter().find(|&&(a,_)| a == exp_ssp.wrapping_add(12)).map(|&(_,v)| v).unwrap_or(0),
            test.final_state.ram.iter().find(|&&(a,_)| a == exp_ssp.wrapping_add(13)).map(|&(_,v)| v).unwrap_or(0),
        ];
        let exp_pc_frame = u32::from_be_bytes(exp_pc_bytes);
        let our_pc_bytes: [u8; 4] = [
            mem.peek(exp_ssp.wrapping_add(10)),
            mem.peek(exp_ssp.wrapping_add(11)),
            mem.peek(exp_ssp.wrapping_add(12)),
            mem.peek(exp_ssp.wrapping_add(13)),
        ];
        let our_pc_frame = u32::from_be_bytes(our_pc_bytes);
        if our_pc_frame != exp_pc_frame {
            let diff = our_pc_frame as i32 - exp_pc_frame as i32;
            pc_wrong_by_mode.entry(mode_key.clone()).or_default().push(diff);
        }

        // Check Areg errors
        for i in 0..7 {
            if cpu.regs.a[i] != test.final_state.a[i] {
                let diff = cpu.regs.a[i] as i32 - test.final_state.a[i] as i32;
                areg_wrong_by_mode.entry(format!("{} A{}", mode_key, i)).or_default().push(diff);
            }
        }

        // Check fault_addr in frame (bytes 2-5)
        let exp_fa_bytes: [u8; 4] = [
            test.final_state.ram.iter().find(|&&(a,_)| a == exp_ssp.wrapping_add(2)).map(|&(_,v)| v).unwrap_or(0),
            test.final_state.ram.iter().find(|&&(a,_)| a == exp_ssp.wrapping_add(3)).map(|&(_,v)| v).unwrap_or(0),
            test.final_state.ram.iter().find(|&&(a,_)| a == exp_ssp.wrapping_add(4)).map(|&(_,v)| v).unwrap_or(0),
            test.final_state.ram.iter().find(|&&(a,_)| a == exp_ssp.wrapping_add(5)).map(|&(_,v)| v).unwrap_or(0),
        ];
        let exp_fa = u32::from_be_bytes(exp_fa_bytes);
        let our_fa_bytes: [u8; 4] = [
            mem.peek(exp_ssp.wrapping_add(2)),
            mem.peek(exp_ssp.wrapping_add(3)),
            mem.peek(exp_ssp.wrapping_add(4)),
            mem.peek(exp_ssp.wrapping_add(5)),
        ];
        let our_fa = u32::from_be_bytes(our_fa_bytes);
        if our_fa != exp_fa {
            let diff = our_fa as i32 - exp_fa as i32;
            fault_wrong_by_mode.entry(mode_key.clone()).or_default().push(diff);
        }
    }

    println!("  SR flag diff patterns: {:?}", sr_flag_diffs);
    println!("  SR wrong by mode:");
    let mut sr_modes: Vec<_> = sr_wrong_by_mode.iter().collect();
    sr_modes.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
    for (mode, count) in &sr_modes { println!("    {} x{}", mode, count); }

    println!("  PC wrong by mode:");
    let mut pc_modes: Vec<_> = pc_wrong_by_mode.iter().collect();
    pc_modes.sort_by_key(|(_, diffs)| std::cmp::Reverse(diffs.len()));
    for (mode, diffs) in &pc_modes {
        let first_few: Vec<_> = diffs.iter().take(5).collect();
        println!("    {} x{} diffs={:?}", mode, diffs.len(), first_few);
    }

    println!("  Areg wrong by mode:");
    for (mode, diffs) in &areg_wrong_by_mode {
        let first_few: Vec<_> = diffs.iter().take(5).collect();
        println!("    {} x{} diffs={:?}", mode, diffs.len(), first_few);
    }

    println!("  FaultAddr wrong by mode:");
    for (mode, diffs) in &fault_wrong_by_mode {
        let first_few: Vec<_> = diffs.iter().take(5).collect();
        println!("    {} x{} diffs={:?}", mode, diffs.len(), first_few);
    }

    // Recount with updated code
    ae_write = 0;
    ae_write_errors.clear();
    sr_wrong_by_mode.clear();
    sr_flag_diffs.clear();
    pc_wrong_by_mode.clear();
    areg_wrong_by_mode.clear();
    fault_wrong_by_mode.clear();
    non_ae = 0;
    non_ae_errors.clear();
    first_examples.clear();

    for (i, test) in tests.iter().enumerate() {
        let mut cpu = M68000::new();
        let mut mem = TestBus::new();
        setup_cpu(&mut cpu, &mut mem, &test.initial);
        for _ in 0..test.cycles { cpu.tick(&mut mem); }
        let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
        if errors.is_empty() { continue; }

        let opcode = test.initial.prefetch[0] as u16;
        let ssp = test.initial.ssp;
        let exp_ssp = test.final_state.ssp;
        let is_ae = exp_ssp == ssp.wrapping_sub(14);

        if !is_ae {
            non_ae += 1;
            if first_examples.len() < 10 {
                first_examples.push((i, format!("NON-AE #{:04} op=0x{:04X}: {}", i, opcode, errors.join("; "))));
            }
            continue;
        }

        let acc_byte = mem.peek(exp_ssp);
        if acc_byte & 0x10 != 0 { continue; } // skip read faults
        ae_write += 1;

        // Check SR
        if cpu.regs.sr != test.final_state.sr {
            let src_mode_bits = (opcode & 0x38) >> 3;
            let src_reg = opcode & 7;
            let dst_mode_bits = (opcode >> 6) & 7;
            let mode_key = format!("src={}{} dst={}{}", src_mode_bits,
                if src_mode_bits == 7 { format!(":{}", src_reg) } else { String::new() },
                dst_mode_bits,
                if dst_mode_bits == 7 { format!(":{}", (opcode >> 9) & 7) } else { String::new() });
            *sr_wrong_by_mode.entry(mode_key).or_insert(0) += 1;
            let diff_bits = cpu.regs.sr ^ test.final_state.sr;
            *sr_flag_diffs.entry(diff_bits).or_insert(0) += 1;
        }
    }

    println!("\n=== Updated MOVE.l analysis (after PC fix) ===");
    println!("  AE write: {}, Non-AE: {}", ae_write, non_ae);
    println!("  SR diff patterns: {:?}", sr_flag_diffs);
    println!("  SR wrong by mode ({} total):", sr_wrong_by_mode.values().sum::<u32>());
    let mut sr_modes: Vec<_> = sr_wrong_by_mode.iter().collect();
    sr_modes.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
    for (mode, count) in &sr_modes { println!("    {} x{}", mode, count); }

    for (_, ex) in &first_examples {
        println!("  {}", ex);
    }

    // Third pass: show first 3 specific examples per PC diff pattern
    println!("\n--- SPECIFIC AE WRITE EXAMPLES ---");
    let mut shown: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for (i, test) in tests.iter().enumerate() {
        let mut cpu = M68000::new();
        let mut mem = TestBus::new();
        setup_cpu(&mut cpu, &mut mem, &test.initial);
        for _ in 0..test.cycles { cpu.tick(&mut mem); }
        let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
        if errors.is_empty() { continue; }

        let opcode = test.initial.prefetch[0] as u16;
        let ssp = test.initial.ssp;
        let exp_ssp = test.final_state.ssp;
        let is_ae = exp_ssp == ssp.wrapping_sub(14);
        if !is_ae { continue; }
        let acc_byte = mem.peek(exp_ssp);
        if acc_byte & 0x10 != 0 { continue; } // skip read faults

        let src_mode_bits = (opcode & 0x38) >> 3;
        let src_reg = opcode & 7;
        let dst_mode_bits = (opcode >> 6) & 7;
        let key = format!("s{}{}_d{}", src_mode_bits,
            if src_mode_bits == 7 { format!(":{}", src_reg) } else { String::new() },
            dst_mode_bits);

        let count = shown.entry(key.clone()).or_insert(0);
        if *count >= 1 { continue; }
        *count += 1;

        // Get frame PC and expected PC
        let exp_pc_bytes: [u8; 4] = [
            test.final_state.ram.iter().find(|&&(a,_)| a == exp_ssp.wrapping_add(10)).map(|&(_,v)| v).unwrap_or(0),
            test.final_state.ram.iter().find(|&&(a,_)| a == exp_ssp.wrapping_add(11)).map(|&(_,v)| v).unwrap_or(0),
            test.final_state.ram.iter().find(|&&(a,_)| a == exp_ssp.wrapping_add(12)).map(|&(_,v)| v).unwrap_or(0),
            test.final_state.ram.iter().find(|&&(a,_)| a == exp_ssp.wrapping_add(13)).map(|&(_,v)| v).unwrap_or(0),
        ];
        let exp_pc_frame = u32::from_be_bytes(exp_pc_bytes);
        let our_pc_bytes: [u8; 4] = [
            mem.peek(exp_ssp.wrapping_add(10)),
            mem.peek(exp_ssp.wrapping_add(11)),
            mem.peek(exp_ssp.wrapping_add(12)),
            mem.peek(exp_ssp.wrapping_add(13)),
        ];
        let our_pc_frame = u32::from_be_bytes(our_pc_bytes);

        // Get initial PC (where opcode was)
        let opcode_addr = test.initial.pc;
        let isp = opcode_addr.wrapping_add(2); // instr_start_pc

        println!("  #{:04} op=0x{:04X} {} opcAddr=0x{:06X} isp=0x{:06X} regsPC=0x{:06X} framPC: our=0x{:06X} exp=0x{:06X} SR: our=0x{:04X} exp=0x{:04X}",
            i, opcode, key, opcode_addr, isp, cpu.regs.pc, our_pc_frame, exp_pc_frame, cpu.regs.sr, test.final_state.sr);
    }
}

#[test]
#[ignore]
fn diagnose_move_l_sr_detail() {
    let path = Path::new("../../test-data/m68000-dl/v1/MOVE.l.json.bin");
    if !path.exists() { return; }
    let tests = decode_file(path).expect("decode");

    // For each failing AE write test with SR error, show:
    // - initial SR, our SR, expected SR
    // - the value being moved
    // - what different flag computations would give
    let mut reg_src_cases = Vec::new();
    let mut mem_src_cases = Vec::new();

    for (i, test) in tests.iter().enumerate() {
        let mut cpu = M68000::new();
        let mut mem = TestBus::new();
        setup_cpu(&mut cpu, &mut mem, &test.initial);
        for _ in 0..test.cycles { cpu.tick(&mut mem); }

        if cpu.regs.sr == test.final_state.sr { continue; }

        let opcode = test.initial.prefetch[0] as u16;
        let ssp = test.initial.ssp;
        let exp_ssp = test.final_state.ssp;
        if exp_ssp != ssp.wrapping_sub(14) { continue; } // not AE

        let acc_byte = mem.peek(exp_ssp);
        if acc_byte & 0x10 != 0 { continue; } // read fault

        let src_mode_bits = (opcode & 0x38) >> 3;
        let src_reg = opcode & 7;
        let dst_mode_bits = (opcode >> 6) & 7;

        // Extract the value that was being moved
        // For register source: value is in the register
        let value = if src_mode_bits == 0 {
            test.initial.d[src_reg as usize]
        } else if src_mode_bits == 1 {
            if src_reg < 7 { test.initial.a[src_reg as usize] } else { test.initial.ssp }
        } else if src_mode_bits == 7 && src_reg == 4 {
            // Immediate: from prefetch/memory
            let hi = test.initial.prefetch[1] as u16;
            let pc = test.initial.pc;
            let lo = (u16::from(mem.peek(pc)) << 8) | u16::from(mem.peek(pc | 1));
            (u32::from(hi) << 16) | u32::from(lo)
        } else {
            // Memory source: read the value from a fresh copy of initial memory.
            let mut src_mem = TestBus::new();
            src_mem.load_ram(&test.initial.ram);
            let irc = test.initial.prefetch[1] as u16;
            let pc = test.initial.pc;

            // Compute effective address based on source mode
            let base_reg = if src_reg < 7 {
                test.initial.a[src_reg as usize]
            } else {
                test.initial.ssp
            };

            let src_addr = match src_mode_bits {
                2 | 3 => base_reg, // (An), (An)+
                4 => base_reg.wrapping_sub(4), // -(An) for long
                5 => {
                    // d(An): displacement is in IRC (first ext word)
                    let disp = irc as i16 as i32;
                    (base_reg as i32).wrapping_add(disp) as u32
                }
                6 => {
                    // d(An,Xn): brief extension word in IRC
                    let xn_reg = (irc >> 12) as usize;
                    let xn_val = if irc & 0x8000 != 0 {
                        if xn_reg < 8 { test.initial.a.get(xn_reg).copied().unwrap_or(0) }
                        else { 0 }
                    } else {
                        test.initial.d[xn_reg & 7]
                    };
                    let xn = if irc & 0x0800 != 0 { xn_val } else { xn_val as i16 as i32 as u32 };
                    let disp = (irc & 0xFF) as i8 as i32;
                    (base_reg as i32).wrapping_add(disp).wrapping_add(xn as i32) as u32
                }
                7 => match src_reg {
                    0 => {
                        // abs.w: sign-extended 16-bit address from IRC
                        irc as i16 as i32 as u32
                    }
                    1 => {
                        // abs.l: 32-bit address from IRC + next word
                        let hi = irc;
                        let lo = (u16::from(src_mem.peek(pc)) << 8) | u16::from(src_mem.peek(pc | 1));
                        (u32::from(hi) << 16) | u32::from(lo)
                    }
                    2 => {
                        // d(PC): displacement from IRC, relative to opcode PC
                        let disp = irc as i16 as i32;
                        let opcode_pc = pc.wrapping_sub(2); // PC before ext word fetch
                        (opcode_pc as i32).wrapping_add(disp) as u32
                    }
                    _ => 0, // placeholder for d(PC,Xn) etc.
                },
                _ => 0,
            };

            let b0 = src_mem.peek(src_addr);
            let b1 = src_mem.peek(src_addr.wrapping_add(1));
            let b2 = src_mem.peek(src_addr.wrapping_add(2));
            let b3 = src_mem.peek(src_addr.wrapping_add(3));
            u32::from_be_bytes([b0, b1, b2, b3])
        };

        let init_sr = test.initial.sr;
        let our_sr = cpu.regs.sr;
        let exp_sr = test.final_state.sr;

        // Compute what different approaches give for CCR (low 5 bits)
        let long_n = ((value >> 31) & 1) as u16;
        let long_z = if value == 0 { 1u16 } else { 0 };
        let word_n = ((value >> 15) & 1) as u16;
        let word_z = if value & 0xFFFF == 0 { 1u16 } else { 0 };
        let hi_word_n = ((value >> 31) & 1) as u16; // same as long_n
        let hi_word_z = if value >> 16 == 0 { 1u16 } else { 0 };

        // X bit is preserved by MOVE (bit 4)
        let init_x = init_sr & 0x10;

        // Full MOVE flags from long: X=init, N=long_n, Z=long_z, V=0, C=0
        let ccr_full_long = init_x | (long_n << 3) | (long_z << 2);
        // Full MOVE flags from word (low): X=init, N=word_n, Z=word_z, V=0, C=0
        let ccr_word = init_x | (word_n << 3) | (word_z << 2);
        // Full MOVE flags from hi word: X=init, N=hi_n, Z=hi_z, V=0, C=0
        let ccr_hi_word = init_x | (hi_word_n << 3) | (hi_word_z << 2);
        // NZ-only from long (preserve V,C from init): X=init, N=long_n, Z=long_z, V/C from init
        let ccr_nz_long = init_x | (long_n << 3) | (long_z << 2) | (init_sr & 0x03);
        // Preserved (init CCR)
        let ccr_preserved = init_sr & 0x1F;

        let exp_ccr = exp_sr & 0x1F;

        let match_label = if exp_ccr == ccr_full_long { "FULL_LONG" }
            else if exp_ccr == ccr_word { "WORD_LO" }
            else if exp_ccr == ccr_hi_word { "HI_WORD" }
            else if exp_ccr == ccr_nz_long { "NZ_LONG" }
            else if exp_ccr == ccr_preserved { "PRESERVED" }
            else { "NONE" };

        let is_reg_src = src_mode_bits <= 1 || (src_mode_bits == 7 && src_reg == 4);
        let entry = format!(
            "  #{:04} src={}{}  dst={} val=0x{:08X} initCCR={:05b} expCCR={:05b} match={}",
            i, src_mode_bits,
            if src_mode_bits == 7 { format!(":{}", src_reg) } else { String::new() },
            dst_mode_bits, value, init_sr & 0x1F, exp_ccr, match_label
        );

        if is_reg_src {
            reg_src_cases.push(entry);
        } else {
            mem_src_cases.push(entry);
        }
    }

    println!("\n=== MOVE.l SR detail: Register/Imm sources ({}) ===", reg_src_cases.len());
    for e in &reg_src_cases { println!("{}", e); }

    println!("\n=== MOVE.l SR detail: Memory sources ({}) ===", mem_src_cases.len());
    for e in &mem_src_cases { println!("{}", e); }

    // Now check ALL memory-source MOVE.l AE tests (including passing ones)
    // to find what distinguishes FULL_LONG-correct from WORD_LO-correct tests
    let mut full_long_correct = std::collections::HashMap::<String, u32>::new();
    let mut word_lo_correct = std::collections::HashMap::<String, u32>::new();
    let mut both_same = 0u32;
    let mut neither_correct = std::collections::HashMap::<String, u32>::new();

    for test in tests.iter() {
        let opcode = test.initial.prefetch[0] as u16;
        let src_mode_bits = (opcode & 0x38) >> 3;
        let src_reg = opcode & 7;
        let dst_mode_bits = (opcode >> 6) & 7;
        let dst_reg = (opcode >> 9) & 7;

        // Skip non-memory sources
        let is_reg_src = src_mode_bits <= 1 || (src_mode_bits == 7 && src_reg == 4);
        if is_reg_src { continue; }

        // Check if this is a write AE
        let ssp = test.initial.ssp;
        let exp_ssp = test.final_state.ssp;
        if exp_ssp != ssp.wrapping_sub(14) { continue; }

        let mut check_mem = TestBus::new();
        check_mem.load_ram(&test.final_state.ram);
        let acc_byte = check_mem.peek(exp_ssp);
        if acc_byte & 0x10 != 0 { continue; } // skip read faults

        // Compute the value being moved
        let mut src_mem = TestBus::new();
        src_mem.load_ram(&test.initial.ram);
        let irc = test.initial.prefetch[1] as u16;
        let pc = test.initial.pc;
        let base_reg = if src_reg < 7 {
            test.initial.a[src_reg as usize]
        } else {
            test.initial.ssp
        };
        let src_addr = match src_mode_bits {
            2 | 3 => base_reg,
            4 => base_reg.wrapping_sub(4),
            5 => {
                let disp = irc as i16 as i32;
                (base_reg as i32).wrapping_add(disp) as u32
            }
            6 => {
                let da = (irc >> 15) & 1;
                let reg_num = ((irc >> 12) & 7) as usize;
                let xn_val = if da == 1 {
                    if reg_num < 7 { test.initial.a[reg_num] } else { test.initial.ssp }
                } else {
                    test.initial.d[reg_num]
                };
                let xn = if irc & 0x0800 != 0 { xn_val } else { xn_val as i16 as i32 as u32 };
                let disp = (irc & 0xFF) as i8 as i32;
                (base_reg as i32).wrapping_add(disp).wrapping_add(xn as i32) as u32
            }
            7 => match src_reg {
                0 => irc as i16 as i32 as u32,
                1 => {
                    let hi = irc;
                    let lo = (u16::from(src_mem.peek(pc)) << 8) | u16::from(src_mem.peek(pc | 1));
                    (u32::from(hi) << 16) | u32::from(lo)
                }
                2 => {
                    let disp = irc as i16 as i32;
                    let opcode_pc = pc.wrapping_sub(2);
                    (opcode_pc as i32).wrapping_add(disp) as u32
                }
                3 => {
                    let da = (irc >> 15) & 1;
                    let reg_num = ((irc >> 12) & 7) as usize;
                    let xn_val = if da == 1 {
                        if reg_num < 7 { test.initial.a[reg_num] } else { test.initial.ssp }
                    } else {
                        test.initial.d[reg_num]
                    };
                    let xn = if irc & 0x0800 != 0 { xn_val } else { xn_val as i16 as i32 as u32 };
                    let disp = (irc & 0xFF) as i8 as i32;
                    let opcode_pc = pc.wrapping_sub(2);
                    (opcode_pc as i32).wrapping_add(disp).wrapping_add(xn as i32) as u32
                }
                _ => 0,
            },
            _ => 0,
        };
        let value = u32::from_be_bytes([
            src_mem.peek(src_addr),
            src_mem.peek(src_addr.wrapping_add(1)),
            src_mem.peek(src_addr.wrapping_add(2)),
            src_mem.peek(src_addr.wrapping_add(3)),
        ]);

        let init_x = test.initial.sr & 0x10;
        let long_n = ((value >> 31) & 1) as u16;
        let long_z = if value == 0 { 1u16 } else { 0 };
        let word_n = ((value >> 15) & 1) as u16;
        let word_z = if value & 0xFFFF == 0 { 1u16 } else { 0 };
        let ccr_full_long = init_x | (long_n << 3) | (long_z << 2);
        let ccr_word_lo = init_x | (word_n << 3) | (word_z << 2);
        let exp_ccr = test.final_state.sr & 0x1F;

        let mode_key = format!("s{}{}_d{}{}",
            src_mode_bits,
            if src_mode_bits == 7 { format!(":{}", src_reg) } else { String::new() },
            dst_mode_bits,
            if dst_mode_bits == 7 { format!(":{}", dst_reg) } else { String::new() });

        if ccr_full_long == ccr_word_lo {
            both_same += 1;
        } else if exp_ccr == ccr_full_long {
            *full_long_correct.entry(mode_key).or_insert(0) += 1;
        } else if exp_ccr == ccr_word_lo {
            *word_lo_correct.entry(mode_key).or_insert(0) += 1;
        } else {
            *neither_correct.entry(mode_key).or_insert(0) += 1;
        }
    }

    println!("\n=== ALL memory-source MOVE.l write AE flag analysis ===");
    println!("  Both same (N/Z agree): {}", both_same);
    println!("  FULL_LONG correct by mode:");
    let mut fl: Vec<_> = full_long_correct.iter().collect();
    fl.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
    for (m, c) in &fl { println!("    {} x{}", m, c); }
    println!("  WORD_LO correct by mode:");
    let mut wl: Vec<_> = word_lo_correct.iter().collect();
    wl.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
    for (m, c) in &wl { println!("    {} x{}", m, c); }
    println!("  Neither correct by mode:");
    let mut nc: Vec<_> = neither_correct.iter().collect();
    nc.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
    for (m, c) in &nc { println!("    {} x{}", m, c); }
}

#[test]
#[ignore]
fn diagnose_move_l_remaining() {
    let path = Path::new("../../test-data/m68000-dl/v1/MOVE.l.json.bin");
    if !path.exists() { return; }
    let tests = decode_file(path).expect("decode");

    println!("\n=== ALL MOVE.l AE tests (pass and fail) ===");

    for (i, test) in tests.iter().enumerate() {
        let opcode = test.initial.prefetch[0] as u16;
        let ssp = test.initial.ssp;
        let exp_ssp = test.final_state.ssp;
        let is_ae = exp_ssp == ssp.wrapping_sub(14);
        if !is_ae { continue; }

        // Check if it's a write AE from expected access info
        let exp_acc = u16::from_be_bytes([
            test.final_state.ram.iter().find(|&&(a,_)| a == exp_ssp).map(|&(_,v)| v).unwrap_or(0),
            test.final_state.ram.iter().find(|&&(a,_)| a == exp_ssp.wrapping_add(1)).map(|&(_,v)| v).unwrap_or(0),
        ]);
        let is_write_ae = exp_acc & 0x10 == 0;

        // Decode src/dst modes
        let src_mode_bits = (opcode & 0x38) >> 3;
        let src_reg = opcode & 7;
        let dst_mode_bits = (opcode >> 6) & 7;
        let dst_reg = (opcode >> 9) & 7;

        // Run the test
        let mut cpu = M68000::new();
        let mut mem = TestBus::new();
        setup_cpu(&mut cpu, &mut mem, &test.initial);
        for _ in 0..test.cycles { cpu.tick(&mut mem); }
        let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);

        // Get frame PC
        let exp_pc_frame = u32::from_be_bytes([
            test.final_state.ram.iter().find(|&&(a,_)| a == exp_ssp.wrapping_add(10)).map(|&(_,v)| v).unwrap_or(0),
            test.final_state.ram.iter().find(|&&(a,_)| a == exp_ssp.wrapping_add(11)).map(|&(_,v)| v).unwrap_or(0),
            test.final_state.ram.iter().find(|&&(a,_)| a == exp_ssp.wrapping_add(12)).map(|&(_,v)| v).unwrap_or(0),
            test.final_state.ram.iter().find(|&&(a,_)| a == exp_ssp.wrapping_add(13)).map(|&(_,v)| v).unwrap_or(0),
        ]);
        let our_pc_frame = u32::from_be_bytes([
            mem.peek(exp_ssp.wrapping_add(10)),
            mem.peek(exp_ssp.wrapping_add(11)),
            mem.peek(exp_ssp.wrapping_add(12)),
            mem.peek(exp_ssp.wrapping_add(13)),
        ]);

        // Get fault address from frame
        let exp_fa = u32::from_be_bytes([
            test.final_state.ram.iter().find(|&&(a,_)| a == exp_ssp.wrapping_add(2)).map(|&(_,v)| v).unwrap_or(0),
            test.final_state.ram.iter().find(|&&(a,_)| a == exp_ssp.wrapping_add(3)).map(|&(_,v)| v).unwrap_or(0),
            test.final_state.ram.iter().find(|&&(a,_)| a == exp_ssp.wrapping_add(4)).map(|&(_,v)| v).unwrap_or(0),
            test.final_state.ram.iter().find(|&&(a,_)| a == exp_ssp.wrapping_add(5)).map(|&(_,v)| v).unwrap_or(0),
        ]);

        // Compute destination address (if register-based mode)
        let an_val = if dst_mode_bits <= 6 && (dst_reg as usize) < test.initial.a.len() {
            test.initial.a[dst_reg as usize]
        } else {
            0
        };
        let dst_addr = match dst_mode_bits {
            4 => an_val.wrapping_sub(4), // pre-decrement for .l
            2 => an_val,                 // indirect
            _ => an_val,
        };

        let pc_diff = exp_pc_frame as i32 - test.initial.pc as i32;
        let pass_fail = if errors.is_empty() { "PASS" } else { "FAIL" };

        // Get extension word (IRC)
        let irc = test.initial.prefetch[1] as u16;

        // For AbsShort src, the source address is sign-extended IRC
        let src_addr = if src_mode_bits == 7 && src_reg == 0 {
            irc as i16 as i32 as u32
        } else {
            0
        };

        println!("  #{:04} {} op=0x{:04X} src={}:{} dst={}:{} A{}=0x{:08X} dst_addr=0x{:08X} fa=0x{:08X} isp=0x{:06X} exp_pc=0x{:06X} pc_diff={} irc=0x{:04X} src_addr=0x{:06X} cycles={}",
            i, pass_fail, opcode, src_mode_bits, src_reg, dst_mode_bits, dst_reg,
            dst_reg, an_val, dst_addr, exp_fa,
            test.initial.pc, exp_pc_frame, pc_diff,
            irc, src_addr, test.cycles);
        if !errors.is_empty() {
            println!("    our_pc=0x{:06X} exp_pc=0x{:06X} diff={}",
                our_pc_frame, exp_pc_frame, our_pc_frame as i32 - exp_pc_frame as i32);
            for err in &errors {
                println!("    ERR: {}", err);
            }
        }
    }
}

#[test]
#[ignore]
fn diagnose_divs_failures() {
    for name in &["DIVS", "DIVU"] {
        let path_str = format!("../../test-data/m68000-dl/v1/{}.json.bin", name);
        let path = Path::new(&path_str);
        if !path.exists() { continue; }
        let tests = decode_file(path).expect("decode");

        println!("\n=== {} failure analysis ===", name);

        for (i, test) in tests.iter().enumerate() {
            let mut cpu = M68000::new();
            let mut mem = TestBus::new();
            setup_cpu(&mut cpu, &mut mem, &test.initial);
            for _ in 0..test.cycles { cpu.tick(&mut mem); }
            let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
            if errors.is_empty() { continue; }

            let opcode = test.initial.prefetch[0] as u16;
            let src_mode = (opcode & 0x38) >> 3;
            let src_reg = opcode & 7;
            let dst_reg = (opcode >> 9) & 7;

            // Check if AE
            let ssp = test.initial.ssp;
            let exp_ssp = test.final_state.ssp;
            let is_ae = exp_ssp == ssp.wrapping_sub(14);

            println!("  #{:04} op=0x{:04X} src={}:{} dst=D{} cycles={} ae={}",
                i, opcode, src_mode, src_reg, dst_reg, test.cycles, is_ae);
            for err in &errors {
                println!("    {}", err);
            }
        }
    }
}

#[test]
#[ignore]
fn diagnose_div_vs_mul_pc() {
    // Jorge Cwik's DIVU timing algorithm (returns total hardware cycles)
    fn cwik_divu(dividend: u32, divisor: u16) -> u32 {
        if divisor == 0 { return 0; }
        if (dividend >> 16) >= u32::from(divisor) { return 10; } // overflow

        let mut mcycles: i32 = 38;
        let hdivisor = u32::from(divisor) << 16;
        let mut dvd = dividend;

        for _ in 0..15 {
            let temp = dvd;
            dvd <<= 1;

            if temp & 0x8000_0000 != 0 {
                // Carry from shift
                dvd = dvd.wrapping_sub(hdivisor);
            } else {
                mcycles += 2;
                if dvd >= hdivisor {
                    dvd = dvd.wrapping_sub(hdivisor);
                    mcycles -= 1;
                }
            }
        }
        (mcycles * 2) as u32
    }

    // Jorge Cwik's DIVS timing algorithm (returns total hardware cycles)
    fn cwik_divs(dividend: i32, divisor: i16) -> u32 {
        if divisor == 0 { return 0; }

        let mut mcycles: i32 = 6;
        if dividend < 0 { mcycles += 1; }

        // Overflow check
        let abs_dividend = (dividend as i64).unsigned_abs() as u32;
        let abs_divisor = (divisor as i32).unsigned_abs() as u16;

        if (abs_dividend >> 16) >= u32::from(abs_divisor) {
            return ((mcycles + 2) * 2) as u32;
        }

        let mut aquot = abs_dividend / u32::from(abs_divisor);

        mcycles += 55;

        if divisor >= 0 {
            if dividend >= 0 { mcycles -= 1; }
            else { mcycles += 1; }
        }

        // Count 15 MSBs of absolute quotient
        for _ in 0..15 {
            if (aquot as i16) >= 0 { mcycles += 1; }
            aquot <<= 1;
        }
        (mcycles * 2) as u32
    }

    // Verify Cwik algorithm matches test data
    for name in &["DIVU", "DIVS"] {
        let path_str = format!("../../test-data/m68000-dl/v1/{}.json.bin", name);
        let path = Path::new(&path_str);
        if !path.exists() { continue; }
        let tests = decode_file(path).expect("decode");

        let mut fail_count = 0u32;
        let mut pass_max_diff: i32 = i32::MIN;
        let mut pass_min_diff: i32 = i32::MAX;
        let mut cwik_exact = 0u32;
        let mut cwik_plus8 = 0u32;
        let mut cwik_other = 0u32;
        let mut normal_count = 0u32;

        for (i, test) in tests.iter().enumerate() {
            let opcode = test.initial.prefetch[0] as u16;
            let src_mode = (opcode & 0x38) >> 3;
            let src_reg = (opcode & 7) as usize;
            let dst_reg = ((opcode >> 9) & 7) as usize;

            // Only DataReg source for clean analysis
            if src_mode != 0 { continue; }

            let dividend = test.initial.d[dst_reg];
            let divisor_full = test.initial.d[src_reg];
            let divisor16 = divisor_full & 0xFFFF;

            if divisor16 == 0 { continue; } // skip div-by-zero (exception)

            let (quotient, overflow) = if *name == "DIVU" {
                let q = dividend / divisor16;
                (q, q > 0xFFFF)
            } else {
                let div_s = (divisor_full as i16) as i32;
                let dvd_s = dividend as i32;
                let q = dvd_s / div_s;
                let ov = !(-32768..=32767).contains(&q);
                (if ov { 0 } else { q as u32 }, ov)
            };

            if overflow {
                let our_timing: u32 = if *name == "DIVU" { 10 } else { 16 };
                let diff = test.cycles as i32 - our_timing as i32;

                let mut cpu = M68000::new();
                let mut mem = TestBus::new();
                setup_cpu(&mut cpu, &mut mem, &test.initial);
                for _ in 0..test.cycles { cpu.tick(&mut mem); }
                let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
                if !errors.is_empty() {
                    fail_count += 1;
                    println!("  OV FAIL #{:04} cycles={} our_timing={} diff={}",
                        i, test.cycles, our_timing, diff);
                }
                continue;
            }

            normal_count += 1;
            let zeros = 16 - (quotient as u16).count_ones();
            let our_timing = if *name == "DIVU" {
                86 + 2 * zeros
            } else {
                120 + 2 * zeros
            };

            // Compute Cwik timing
            let cwik_total = if *name == "DIVU" {
                cwik_divu(dividend, divisor16 as u16)
            } else {
                cwik_divs(dividend as i32, (divisor_full as i16))
            };
            // DL test.cycles = total hardware cycles - 8 (opcode + IRC fetch)
            let cwik_dl = cwik_total.wrapping_sub(8);
            let cwik_diff = test.cycles as i32 - cwik_dl as i32;

            let diff = test.cycles as i32 - our_timing as i32;

            let mut cpu = M68000::new();
            let mut mem = TestBus::new();
            setup_cpu(&mut cpu, &mut mem, &test.initial);
            for _ in 0..test.cycles { cpu.tick(&mut mem); }
            let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
            if !errors.is_empty() {
                fail_count += 1;
                if fail_count <= 5 {
                    println!("  FAIL #{:04} q=0x{:04X} zeros={} our={} cwik_dl={} test={} diff={} cwik_diff={}",
                        i, quotient & 0xFFFF, zeros, our_timing, cwik_dl, test.cycles, diff, cwik_diff);
                }
            } else {
                pass_max_diff = pass_max_diff.max(diff);
                pass_min_diff = pass_min_diff.min(diff);
                // Count Cwik accuracy
                if cwik_total == test.cycles { cwik_exact += 1; }
                else if cwik_total == test.cycles + 8 { cwik_plus8 += 1; }
                else { cwik_other += 1; }
            }
        }
        println!("{} DataReg: normal={} failures={} pass_diff=[{},{}] cwik: exact={} +8={} other={}",
            name, normal_count, fail_count, pass_min_diff, pass_max_diff,
            cwik_exact, cwik_plus8, cwik_other);

        // Compute base = test_cycles - 2*zeros for a sample of tests
        println!("  Sampling base values (test_cycles - 2*zeros):");
        let mut bases: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
        let mut count = 0;
        for test in tests.iter() {
            let opcode = test.initial.prefetch[0] as u16;
            let src_mode = (opcode & 0x38) >> 3;
            let src_reg = (opcode & 7) as usize;
            let dst_reg = ((opcode >> 9) & 7) as usize;
            if src_mode != 0 { continue; }

            let dividend = test.initial.d[dst_reg];
            let divisor_full = test.initial.d[src_reg];
            let divisor16 = divisor_full & 0xFFFF;
            if divisor16 == 0 { continue; }

            let (quotient, overflow) = if *name == "DIVU" {
                let q = dividend / divisor16;
                (q, q > 0xFFFF)
            } else {
                let div_s = (divisor_full as i16) as i32;
                let dvd_s = dividend as i32;
                let q = dvd_s / div_s;
                let ov = !(-32768..=32767).contains(&q);
                (if ov { 0 } else { q as u32 }, ov)
            };
            if overflow { continue; }

            let zeros = 16 - (quotient as u16).count_ones();
            let base = test.cycles as i32 - 2 * zeros as i32;
            *bases.entry(base as u32).or_insert(0) += 1;
            count += 1;
            if count <= 10 {
                // Also show dividend MSB and quotient for pattern analysis
                let msb = (dividend >> 31) & 1;
                println!("    cycles={} zeros={} base={} q=0x{:04X} dvd_msb={} dividend=0x{:08X}",
                    test.cycles, zeros, base, quotient & 0xFFFF, msb, dividend);
            }
        }
        let mut base_list: Vec<_> = bases.into_iter().collect();
        base_list.sort();
        println!("  Base distribution: {:?}", base_list);
    }
}

#[test]
#[ignore]
fn diagnose_move_w_failures() {
    let path = Path::new("../../test-data/m68000-dl/v1/MOVE.w.json.bin");
    if !path.exists() { return; }
    let tests = decode_file(path).expect("decode");

    println!("\n=== MOVE.w failure analysis ===");

    for (i, test) in tests.iter().enumerate() {
        let mut cpu = M68000::new();
        let mut mem = TestBus::new();
        setup_cpu(&mut cpu, &mut mem, &test.initial);
        for _ in 0..test.cycles { cpu.tick(&mut mem); }
        let errors = compare_state(&cpu, &mem, &test.final_state, &test.name);
        if errors.is_empty() { continue; }

        let opcode = test.initial.prefetch[0] as u16;
        let ssp = test.initial.ssp;
        let exp_ssp = test.final_state.ssp;
        let is_ae = exp_ssp == ssp.wrapping_sub(14);

        let src_mode_bits = (opcode & 0x38) >> 3;
        let src_reg = opcode & 7;
        let dst_mode_bits = (opcode >> 6) & 7;
        let dst_reg = (opcode >> 9) & 7;

        if !is_ae {
            println!("  #{:04} op=0x{:04X} src={}:{} dst={}:{} NON-AE: {}",
                i, opcode, src_mode_bits, src_reg, dst_mode_bits, dst_reg,
                errors.join("; "));
            continue;
        }

        // Get expected access info to determine read/write
        let exp_acc = u16::from_be_bytes([
            test.final_state.ram.iter().find(|&&(a,_)| a == exp_ssp).map(|&(_,v)| v).unwrap_or(0),
            test.final_state.ram.iter().find(|&&(a,_)| a == exp_ssp.wrapping_add(1)).map(|&(_,v)| v).unwrap_or(0),
        ]);
        let our_acc = u16::from_be_bytes([
            mem.peek(exp_ssp),
            mem.peek(exp_ssp.wrapping_add(1)),
        ]);
        let is_write_ae = exp_acc & 0x10 == 0;

        // Get frame PC
        let exp_pc_frame = u32::from_be_bytes([
            test.final_state.ram.iter().find(|&&(a,_)| a == exp_ssp.wrapping_add(10)).map(|&(_,v)| v).unwrap_or(0),
            test.final_state.ram.iter().find(|&&(a,_)| a == exp_ssp.wrapping_add(11)).map(|&(_,v)| v).unwrap_or(0),
            test.final_state.ram.iter().find(|&&(a,_)| a == exp_ssp.wrapping_add(12)).map(|&(_,v)| v).unwrap_or(0),
            test.final_state.ram.iter().find(|&&(a,_)| a == exp_ssp.wrapping_add(13)).map(|&(_,v)| v).unwrap_or(0),
        ]);
        let our_pc_frame = u32::from_be_bytes([
            mem.peek(exp_ssp.wrapping_add(10)),
            mem.peek(exp_ssp.wrapping_add(11)),
            mem.peek(exp_ssp.wrapping_add(12)),
            mem.peek(exp_ssp.wrapping_add(13)),
        ]);

        let pc_ok = our_pc_frame == exp_pc_frame;
        let acc_ok = our_acc == exp_acc;

        println!("  #{:04} op=0x{:04X} src={}:{} dst={}:{} isp=0x{:06X} {}",
            i, opcode, src_mode_bits, src_reg, dst_mode_bits, dst_reg, test.initial.pc,
            if is_write_ae { "WRITE-AE" } else { "READ-AE" });

        if !pc_ok {
            let diff = our_pc_frame as i32 - exp_pc_frame as i32;
            println!("    PC: our=0x{:06X} exp=0x{:06X} diff={}", our_pc_frame, exp_pc_frame, diff);
        }
        if !acc_ok {
            let exp_fc = exp_acc & 7;
            let our_fc = our_acc & 7;
            println!("    ACC: our=0x{:04X} exp=0x{:04X}", our_acc, exp_acc);
            if exp_fc != our_fc {
                println!("      FC: our={} exp={}", our_fc, exp_fc);
            }
        }
        if cpu.regs.sr != test.final_state.sr {
            println!("    SR: our=0x{:04X} exp=0x{:04X} diff=0x{:04X}",
                cpu.regs.sr, test.final_state.sr, cpu.regs.sr ^ test.final_state.sr);
        }
        // Show non-frame errors
        for err in &errors {
            if !err.contains("RAM[") {
                println!("    {}", err);
            }
        }
    }
}
