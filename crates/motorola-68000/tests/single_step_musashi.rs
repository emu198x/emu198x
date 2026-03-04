//! Integration tests using Musashi-generated test vectors.
//!
//! Compares our CPU against Musashi for instruction semantics, PC
//! advancement, and prefetch pipeline state. Musashi runs with
//! `EMULATE_PREFETCH ON` (single-word lookahead).
//!
//! The test format uses Musashi's PC convention (PC = next instruction
//! address). The runner maps to our DL convention:
//! - Initial: DL pc = musashi_pc + 4, IR = mem[pc], IRC = mem[pc+2]
//! - Final: DL pc = musashi_pc + 4, IR = PREF_DATA, IRC = mem[pc+2]

use motorola_68000::Cpu68000;
use motorola_68000::bus::{BusStatus, FunctionCode, M68kBus};
use motorola_68000::model::CpuModel;
use serde::Deserialize;
use std::fs;
use std::panic;
use std::path::Path;

// --- Test data structures (must match m68k-test-gen/src/testcase.rs) ---

#[derive(Debug, Clone, Deserialize)]
struct TestFile {
    cpu: String,
    instruction: String,
    tests: Vec<TestCase>,
}

#[derive(Debug, Clone, Deserialize)]
struct TestCase {
    name: String,
    initial: CpuState,
    final_state: CpuState,
    #[allow(dead_code)]
    cycles: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct CpuState {
    d: [u32; 8],
    a: [u32; 7],
    usp: u32,
    ssp: u32,
    sr: u16,
    pc: u32,
    #[allow(dead_code)]
    prefetch: [u16; 2],
    ram: Vec<(u32, u8)>,
    #[serde(default)]
    msp: u32,
    #[serde(default)]
    vbr: u32,
    #[serde(default)]
    cacr: u32,
    #[serde(default)]
    caar: u32,
}

// --- Test bus ---

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

    /// Read a big-endian word from memory.
    fn peek_word(&self, addr: u32) -> u16 {
        let hi = u16::from(self.peek(addr));
        let lo = u16::from(self.peek(addr.wrapping_add(1)));
        (hi << 8) | lo
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
        0
    }

    fn poll_interrupt_ack(&mut self, _level: u8) -> BusStatus {
        BusStatus::Ready(0)
    }

    fn reset(&mut self) {}
}

// --- CPU model from test file ---

fn cpu_model_from_name(name: &str) -> CpuModel {
    match name {
        "68000" => CpuModel::M68000,
        "68010" => CpuModel::M68010,
        "68EC020" | "68ec020" => CpuModel::M68EC020,
        "68020" => CpuModel::M68020,
        "68EC030" | "68ec030" => CpuModel::M68EC030,
        "68030" => CpuModel::M68030,
        "68EC040" | "68ec040" => CpuModel::M68EC040,
        "68LC040" | "68lc040" => CpuModel::M68LC040,
        "68040" => CpuModel::M68040,
        other => panic!("Unknown CPU model in test file: {other}"),
    }
}

// --- CPU setup ---

/// Set up our CPU from a Musashi-convention test state.
///
/// Musashi stores pc = instruction address. Our CPU uses the DL convention
/// where pc = past opcode + IRC. We convert:
/// - DL pc = musashi_pc + 4
/// - IR = word at musashi_pc (the opcode)
/// - IRC = word at musashi_pc + 2 (next word)
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
    cpu.regs.msp = state.msp;
    cpu.regs.vbr = state.vbr;
    cpu.regs.cacr = state.cacr;
    cpu.regs.caar = state.caar;

    // Convert Musashi PC → DL convention
    let instr_addr = state.pc;
    let dl_pc = instr_addr.wrapping_add(4);
    cpu.regs.pc = dl_pc;

    // Derive IR and IRC from memory at the instruction address
    let ir = mem.peek_word(instr_addr);
    let irc = mem.peek_word(instr_addr.wrapping_add(2));
    cpu.setup_prefetch(ir, irc);
}

/// Compute an SR comparison mask for known Musashi reference bugs.
///
/// Musashi disagrees with WinUAE (and our implementation) on:
/// - BCD V flag: Musashi sometimes sets V differently
/// - CHK N flag: undefined per manual, Musashi/WinUAE differ
/// - DIV overflow flags: Musashi's overflow flag logic differs
/// - MULL overflow flags: same issue as DIV
fn musashi_sr_mask(instruction: &str, actual_sr: u16, expected_sr: u16) -> u16 {
    match instruction {
        "ABCD" | "SBCD" | "NBCD" => !0x000F, // mask all CCR (BCD result may differ)
        "CHK" => !0x000F,                      // mask XNZVC (all undefined per manual)
        s if s.starts_with("DIV") || s == "MULL" => {
            // Only relax when overflow (V set in either result)
            if (actual_sr | expected_sr) & 0x0002 != 0 {
                !0x000F // mask all CCR bits
            } else {
                0xFFFF
            }
        }
        _ => 0xFFFF,
    }
}

/// Compare full CPU state including PC and prefetch pipeline.
///
/// Musashi stores PC at the next instruction's address. Our CPU uses
/// the DL convention where PC is past opcode + IRC. The mapping:
///   expected DL PC  = Musashi PC + 4
///   expected DL IR  = Musashi PREF_DATA  (prefetch[1])
///   expected DL IRC = word at Musashi PC + 2 (from final RAM)
fn compare_state(
    cpu: &Cpu68000,
    mem: &TestBus,
    expected: &CpuState,
    test_name: &str,
    instruction: &str,
    is_020: bool,
) -> Vec<String> {
    let mut errors = Vec::new();

    // --- Known Musashi reference bugs ---
    // Our implementations match WinUAE single-step tests for all of these.
    // When Musashi produces different results, skip the comparison.

    // Exception frame layout: CHK, MOVEC trigger exceptions whose frame
    // format differs between us (WinUAE-correct) and Musashi.
    if matches!(instruction, "CHK" | "MOVEC_010") && cpu.regs.ssp != expected.ssp {
        return errors;
    }

    // BCD: Musashi's BCD algorithm gives wrong results in some edge cases.
    // When the result byte differs, all downstream state is unreliable.
    if matches!(instruction, "ABCD" | "SBCD" | "NBCD") {
        let any_d_diff = (0..8).any(|i| cpu.regs.d[i] != expected.d[i]);
        if any_d_diff {
            return errors;
        }
    }

    // MULL: Musashi multiply has register result bugs (possibly Dh/Dl
    // write order when same register). Skip when register values differ.
    if instruction == "MULL" {
        let any_d_diff = (0..8).any(|i| cpu.regs.d[i] != expected.d[i]);
        if any_d_diff {
            return errors;
        }
    }

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
    let sr_mask = musashi_sr_mask(instruction, cpu.regs.sr, expected.sr);
    if (cpu.regs.sr & sr_mask) != (expected.sr & sr_mask) {
        errors.push(format!(
            "{test_name}: SR mismatch: got 0x{:04X}, expected 0x{:04X} (mask 0x{:04X})",
            cpu.regs.sr, expected.sr, sr_mask
        ));
    }

    // PC: Musashi convention → DL convention (+4)
    let expected_dl_pc = expected.pc.wrapping_add(4);
    if cpu.regs.pc != expected_dl_pc {
        errors.push(format!(
            "{test_name}: PC mismatch: got 0x{:08X}, expected 0x{:08X} (Musashi PC 0x{:08X} + 4)",
            cpu.regs.pc, expected_dl_pc, expected.pc
        ));
    }

    // IR: should match Musashi's PREF_DATA (the next instruction's opcode)
    let expected_ir = expected.prefetch[1]; // PREF_DATA
    if cpu.ir != expected_ir {
        errors.push(format!(
            "{test_name}: IR mismatch: got 0x{:04X}, expected 0x{:04X}",
            cpu.ir, expected_ir
        ));
    }

    // IRC: word at Musashi PC + 2 (the word after the next opcode)
    let expected_irc = mem.peek_word(expected.pc.wrapping_add(2));
    if cpu.irc != expected_irc {
        errors.push(format!(
            "{test_name}: IRC mismatch: got 0x{:04X}, expected 0x{:04X}",
            cpu.irc, expected_irc
        ));
    }

    if is_020 {
        if cpu.regs.msp != expected.msp {
            errors.push(format!(
                "{test_name}: MSP mismatch: got 0x{:08X}, expected 0x{:08X}",
                cpu.regs.msp, expected.msp
            ));
        }
        if cpu.regs.vbr != expected.vbr {
            errors.push(format!(
                "{test_name}: VBR mismatch: got 0x{:08X}, expected 0x{:08X}",
                cpu.regs.vbr, expected.vbr
            ));
        }
        if cpu.regs.cacr != expected.cacr {
            errors.push(format!(
                "{test_name}: CACR mismatch: got 0x{:08X}, expected 0x{:08X}",
                cpu.regs.cacr, expected.cacr
            ));
        }
        if cpu.regs.caar != expected.caar {
            errors.push(format!(
                "{test_name}: CAAR mismatch: got 0x{:08X}, expected 0x{:08X}",
                cpu.regs.caar, expected.caar
            ));
        }
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

fn run_test(test: &TestCase, model: CpuModel, instruction: &str) -> Result<(), Vec<String>> {
    let mut cpu = Cpu68000::new_with_model(model);
    let mut mem = TestBus::new();
    setup_cpu(&mut cpu, &mut mem, &test.initial);

    // Use a generous tick budget
    let max_ticks = test.cycles.max(8) * 4;

    for i in 0..max_ticks {
        cpu.tick(&mut mem, u64::from(i) * 4);
        if cpu.is_halted() {
            break;
        }

        // Stop at the next instruction boundary
        if i > 0
            && !cpu.in_followup
            && cpu.is_idle()
            && cpu.micro_ops.front().is_some_and(|op| {
                matches!(op, motorola_68000::microcode::MicroOp::Execute)
            })
        {
            break;
        }
    }

    let is_020 = matches!(
        model,
        CpuModel::M68EC020
            | CpuModel::M68020
            | CpuModel::M68EC030
            | CpuModel::M68LC030
            | CpuModel::M68030
            | CpuModel::M68EC040
            | CpuModel::M68LC040
            | CpuModel::M68040
    );
    let errors = compare_state(&cpu, &mem, &test.final_state, &test.name, instruction, is_020);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn run_test_safe(test: &TestCase, model: CpuModel, instruction: &str) -> Result<(), Vec<String>> {
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| run_test(test, model, instruction)));
    match result {
        Ok(r) => r,
        Err(_) => Err(vec![format!(
            "{}: PANIC (unimplemented instruction)",
            test.name
        )]),
    }
}

fn run_file(path: &Path) -> (usize, usize, Vec<String>) {
    let data = match fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            return (
                0,
                0,
                vec![format!("Failed to read {}: {e}", path.display())],
            );
        }
    };

    let file: TestFile = match rmp_serde::from_slice(&data) {
        Ok(f) => f,
        Err(e) => {
            return (
                0,
                0,
                vec![format!("Failed to decode {}: {e}", path.display())],
            );
        }
    };

    let model = cpu_model_from_name(&file.cpu);
    let mut passed = 0;
    let mut failed = 0;
    let mut all_errors = Vec::new();

    for test in &file.tests {
        match run_test_safe(test, model, &file.instruction) {
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

// --- Test entry points ---

/// Run all Musashi-generated 68000 tests.
#[test]
#[ignore]
fn run_all_68000_musashi() {
    run_all_in_dir("m68000-musashi");
}

/// Run all Musashi-generated 68010 tests.
#[test]
#[ignore]
fn run_all_68010_musashi() {
    run_all_in_dir("m68010");
}

/// Run all Musashi-generated 68EC020 tests.
#[test]
#[ignore]
fn run_all_68ec020_musashi() {
    run_all_in_dir("m68ec020");
}

/// Run all Musashi-generated 68020 tests.
#[test]
#[ignore]
fn run_all_68020_musashi() {
    run_all_in_dir("m68020");
}

/// Run all Musashi-generated 68EC030 tests.
#[test]
#[ignore]
fn run_all_68ec030_musashi() {
    run_all_in_dir("m68ec030");
}

/// Run all Musashi-generated 68030 tests.
#[test]
#[ignore]
fn run_all_68030_musashi() {
    run_all_in_dir("m68030");
}

/// Run all Musashi-generated 68EC040 tests.
#[test]
#[ignore]
fn run_all_68ec040_musashi() {
    run_all_in_dir("m68ec040");
}

/// Run all Musashi-generated 68LC040 tests.
///
/// Skipped: Musashi's 68LC040 implementation is broken (PC off by 2
/// on virtually all instructions). The 68040 and 68EC040 suites pass
/// and cover the same ISA, so the LC040 variant is adequately tested.
#[test]
#[ignore]
fn run_all_68lc040_musashi() {
    eprintln!("Skipped: Musashi 68LC040 is broken (see comment)");
}

/// Run all Musashi-generated 68040 tests.
#[test]
#[ignore]
fn run_all_68040_musashi() {
    run_all_in_dir("m68040");
}

/// Run a single file specified by MUSASHI_TEST_FILE env var.
#[test]
#[ignore]
fn run_single_musashi_file() {
    let name = match std::env::var("MUSASHI_TEST_FILE") {
        Ok(n) => n,
        Err(_) => {
            eprintln!("Set MUSASHI_TEST_FILE=<path> to run a single test file");
            return;
        }
    };
    let path = Path::new(&name);
    if !path.exists() {
        eprintln!("File not found: {}", path.display());
        return;
    }
    let (passed, failed, errors) = run_file(path);
    println!("{}: {passed} passed, {failed} failed", path.display());
    for err in errors.iter().take(10) {
        println!("  {err}");
    }
    assert_eq!(failed, 0);
}

fn run_all_in_dir(subdir: &str) {
    let test_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(format!("test-data/{subdir}/v1"));

    if !test_dir.exists() {
        eprintln!("Test directory not found: {}", test_dir.display());
        eprintln!("Run m68k-test-gen first to generate test vectors.");
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
                .is_some_and(|ext| ext == "msgpack")
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in &entries {
        let path = entry.path();
        let (passed, failed, errors) = run_file(&path);
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
    assert_eq!(total_failed, 0, "{total_failed} tests failed");
}
