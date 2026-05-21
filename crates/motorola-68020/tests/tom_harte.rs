//! Tom Harte-style single-step harness for the 68020.
//!
//! There is no upstream Tom Harte corpus for the 68020 today, so the
//! vectors come from `m68k-test-gen` (in `Emu198x-Oldest/`), which
//! drives Musashi as a reference oracle and emits MessagePack files
//! using the structs defined inline below. The schema is a superset
//! of the 68000 fixture format — the 68020-only registers (`msp`,
//! `vbr`, `cacr`, `caar`) live in extra fields that are zero on 68000
//! fixtures.
//!
//! Default corpus root:
//!   `~/Projects/198x/assets/test-suites/m68k-generated/m68020/v1/`
//!
//! Override with the `M68020_TEST_DATA` environment variable.
//!
//! Each fixture covers one instruction (or one addressing-mode variant
//! of an instruction). The current `Cpu68020` type is still aliased to
//! `Cpu68000` — this harness is the baseline measurement that
//! `knowledge/decisions/motorola-68020-implementation-plan.md` Phase 0
//! calls for: the pass rate before any 68020-specific work begins.
//!
//! Skipped under normal `cargo test`. Run explicitly with:
//!
//! ```sh
//! cargo test -p motorola-68020 --test tom_harte -- --ignored --nocapture
//! ```

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use motorola_68000::bus::{BusStatus, FunctionCode};
use motorola_68000::cpu::State;
use motorola_68020::Cpu68020;

// ─── Fixture schema (mirrors m68k-test-gen / testcase.rs) ─────────

/// One MessagePack file = one TestFile (cpu, instruction, tests).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestFile {
    cpu: String,
    instruction: String,
    tests: Vec<TestCase>,
}

/// One test vector: initial → final after a single instruction.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestCase {
    name: String,
    initial: CpuState,
    final_state: CpuState,
    /// Musashi's cycle count — recorded but not compared today.
    cycles: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CpuState {
    d: [u32; 8],
    a: [u32; 7],
    usp: u32,
    /// SSP on 68000, ISP on 68020+.
    ssp: u32,
    sr: u16,
    pc: u32,
    /// [IR, IRC]
    prefetch: [u16; 2],
    ram: Vec<(u32, u8)>,
    // 68020+ supersets — absent on 68000 files, default to zero.
    #[serde(default)]
    msp: u32,
    #[serde(default)]
    vbr: u32,
    #[serde(default)]
    cacr: u32,
    #[serde(default)]
    caar: u32,
}

// ─── Sparse memory ────────────────────────────────────────────────

struct SparseMem {
    bytes: HashMap<u32, u8>,
}

impl SparseMem {
    fn new() -> Self {
        Self {
            bytes: HashMap::new(),
        }
    }

    fn load_ram(&mut self, ram: &[(u32, u8)]) {
        for (addr, value) in ram {
            self.bytes.insert(addr & 0xFF_FFFF, *value);
        }
    }

    fn read_byte(&self, addr: u32) -> u8 {
        *self.bytes.get(&(addr & 0xFF_FFFF)).unwrap_or(&0)
    }

    fn read_word(&self, addr: u32) -> u16 {
        let a = addr & 0xFF_FFFE;
        (u16::from(self.read_byte(a)) << 8) | u16::from(self.read_byte(a + 1))
    }

    fn write_byte(&mut self, addr: u32, val: u8) {
        self.bytes.insert(addr & 0xFF_FFFF, val);
    }

    fn write_word(&mut self, addr: u32, val: u16) {
        let a = addr & 0xFF_FFFE;
        self.write_byte(a, (val >> 8) as u8);
        self.write_byte(a + 1, val as u8);
    }
}

// ─── Fixture loading ──────────────────────────────────────────────

fn candidate_fixture_roots() -> Vec<PathBuf> {
    let home = std::env::var("HOME").expect("HOME set");
    let home = PathBuf::from(home);

    let mut roots = Vec::new();
    if let Ok(path) = std::env::var("M68020_TEST_DATA") {
        roots.push(PathBuf::from(path));
    }
    roots.push(home.join("Projects/198x/assets/test-suites/m68k-generated/m68020/v1"));
    roots
}

fn fixture_root() -> PathBuf {
    candidate_fixture_roots()
        .into_iter()
        .find(|path| path.is_dir())
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").expect("HOME set");
            PathBuf::from(home).join("Projects/198x/assets/test-suites/m68k-generated/m68020/v1")
        })
}

fn load_fixture(path: &Path) -> Option<TestFile> {
    let mut file = File::open(path).ok()?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).ok()?;
    rmp_serde::from_slice(&buf).ok()
}

// ─── CPU state setup ──────────────────────────────────────────────

fn apply_initial(cpu: &mut Cpu68020, mem: &mut SparseMem, initial: &CpuState) {
    cpu.regs.d = initial.d;
    cpu.regs.a = initial.a;
    cpu.regs.usp = initial.usp;
    cpu.regs.ssp = initial.ssp;
    cpu.regs.sr = initial.sr;

    // 68020-only register state (msp / vbr / cacr / caar) is ignored
    // for now — the current Cpu68020 is a type alias of Cpu68000 and
    // has no fields to receive these. They'll start mattering once
    // Phase 1 forks Cpu68020 into its own struct.

    // m68k-test-gen stores Musashi's [IR, PREF_DATA] in initial.prefetch,
    // which is *not* the opcode bytes — Musashi's IR after pulse_reset
    // is usually stale. The opcode and the lookahead word are already
    // poked into initial.ram by `encode_instruction`, so read them
    // straight from memory here and use those as IR/IRC.
    let pc = initial.pc;
    let pf0 = mem.read_word(pc);
    let pf1 = mem.read_word(pc.wrapping_add(2));

    cpu.regs.pc = pc.wrapping_add(4);
    cpu.setup_prefetch(pf0, pf1);
}

// ─── Bus service (same pattern as the 68000 harness) ──────────────

fn service_bus(cpu: &mut Cpu68020, mem: &mut SparseMem) {
    if let State::BusCycle {
        addr,
        fc,
        is_read,
        is_word,
        data,
        cycle_count,
        ..
    } = &cpu.state
    {
        if *cycle_count >= 3 {
            if *fc == FunctionCode::InterruptAck {
                cpu.bus_status = BusStatus::Ready(24 + u16::from(cpu.ipl));
            } else if *is_read {
                let val = if *is_word {
                    mem.read_word(*addr)
                } else {
                    u16::from(mem.read_byte(*addr))
                };
                cpu.bus_status = BusStatus::Ready(val);
            } else {
                let val = data.unwrap_or(0);
                if *is_word {
                    mem.write_word(*addr, val);
                } else {
                    mem.write_byte(*addr, val as u8);
                }
                cpu.bus_status = BusStatus::Ready(0);
            }
        } else {
            cpu.bus_status = BusStatus::Wait;
        }
    } else {
        cpu.bus_status = BusStatus::Wait;
    }
}

fn run_one_instruction(cpu: &mut Cpu68020, mem: &mut SparseMem) -> bool {
    let start_count = cpu.instruction_starts;
    for _ in 0..400 {
        service_bus(cpu, mem);
        cpu.tick();
        if cpu.instruction_starts > start_count {
            return true;
        }
    }
    false
}

// ─── State comparison ─────────────────────────────────────────────

#[derive(Default, Debug)]
struct Mismatch {
    field: String,
    expected: String,
    actual: String,
}

fn compare_final(cpu: &Cpu68020, mem: &SparseMem, final_state: &CpuState) -> Vec<Mismatch> {
    let mut v = Vec::new();

    for i in 0..8 {
        if cpu.regs.d[i] != final_state.d[i] {
            v.push(Mismatch {
                field: format!("d{i}"),
                expected: format!("${:08X}", final_state.d[i]),
                actual: format!("${:08X}", cpu.regs.d[i]),
            });
        }
    }
    for i in 0..7 {
        if cpu.regs.a[i] != final_state.a[i] {
            v.push(Mismatch {
                field: format!("a{i}"),
                expected: format!("${:08X}", final_state.a[i]),
                actual: format!("${:08X}", cpu.regs.a[i]),
            });
        }
    }

    if cpu.regs.usp != final_state.usp {
        v.push(Mismatch {
            field: "usp".into(),
            expected: format!("${:08X}", final_state.usp),
            actual: format!("${:08X}", cpu.regs.usp),
        });
    }
    if cpu.regs.ssp != final_state.ssp {
        v.push(Mismatch {
            field: "ssp".into(),
            expected: format!("${:08X}", final_state.ssp),
            actual: format!("${:08X}", cpu.regs.ssp),
        });
    }

    // Tom Harte "final PC" = address of the next instruction the
    // prefetched word came from. Match against `instr_start_pc`, same
    // as the 68000 harness.
    if cpu.instr_start_pc != final_state.pc {
        v.push(Mismatch {
            field: "pc".into(),
            expected: format!("${:08X}", final_state.pc),
            actual: format!("${:08X}", cpu.instr_start_pc),
        });
    }
    if cpu.regs.sr != final_state.sr {
        v.push(Mismatch {
            field: "sr".into(),
            expected: format!("${:04X}", final_state.sr),
            actual: format!("${:04X}", cpu.regs.sr),
        });
    }

    for (addr, expected) in &final_state.ram {
        let actual = mem.read_byte(*addr);
        if actual != *expected {
            v.push(Mismatch {
                field: format!("mem[${addr:06X}]"),
                expected: format!("${expected:02X}"),
                actual: format!("${actual:02X}"),
            });
        }
    }

    v
}

// ─── Per-fixture runner ────────────────────────────────────────────

struct FixtureResult {
    name: String,
    total: usize,
    passed: usize,
    first_fail: Option<(String, Vec<Mismatch>)>,
}

fn run_fixture(path: &Path) -> Option<FixtureResult> {
    let file = load_fixture(path)?;
    let name = path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .trim_end_matches(".msgpack")
        .to_string();

    let mut passed = 0;
    let mut first_fail: Option<(String, Vec<Mismatch>)> = None;

    for test in &file.tests {
        let mut cpu = Cpu68020::new();
        let mut mem = SparseMem::new();
        mem.load_ram(&test.initial.ram);
        apply_initial(&mut cpu, &mut mem, &test.initial);

        if !run_one_instruction(&mut cpu, &mut mem) {
            if first_fail.is_none() {
                first_fail = Some((
                    test.name.clone(),
                    vec![Mismatch {
                        field: "run".into(),
                        expected: "1 instruction".into(),
                        actual: "timeout".into(),
                    }],
                ));
            }
            continue;
        }

        let mismatches = compare_final(&cpu, &mem, &test.final_state);
        if mismatches.is_empty() {
            passed += 1;
        } else if first_fail.is_none() {
            first_fail = Some((test.name.clone(), mismatches));
        }
    }

    Some(FixtureResult {
        name,
        total: file.tests.len(),
        passed,
        first_fail,
    })
}

fn print_result(r: &FixtureResult) {
    let rate = if r.total > 0 {
        (r.passed as f64 / r.total as f64) * 100.0
    } else {
        0.0
    };
    println!(
        "  {:<32} {:>5}/{:<5} ({:>5.1}%)",
        r.name, r.passed, r.total, rate
    );
    if let Some((case, mismatches)) = &r.first_fail {
        println!("    first fail: {case}");
        for m in mismatches.iter().take(4) {
            println!(
                "      {:<16} expected={:<12} actual={}",
                m.field, m.expected, m.actual
            );
        }
        if mismatches.len() > 4 {
            println!("      ... and {} more", mismatches.len() - 4);
        }
    }
}

// ─── Tests ─────────────────────────────────────────────────────────

/// Baseline sweep: run every fixture in the corpus root and report
/// per-instruction pass rate plus a workspace total. The current
/// `Cpu68020` is still aliased to `Cpu68000`, so anything that
/// requires 68020 semantics (bitfield ops, scaled index, MULL/DIVL,
/// full extension words, …) is expected to fail. The total here is
/// the Phase 0 baseline the implementation plan measures against.
#[test]
#[ignore]
fn harte_baseline_full_sweep() {
    let root = fixture_root();
    if !root.exists() {
        eprintln!("Skipping: fixture dir not found at {}", root.display());
        return;
    }

    let mut entries: Vec<PathBuf> = std::fs::read_dir(&root)
        .expect("read_dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().ends_with(".msgpack"))
                .unwrap_or(false)
        })
        .collect();
    entries.sort();

    println!();
    println!(
        "Tom Harte 68020 baseline ({} fixtures, Cpu68020 = Cpu68000):",
        entries.len()
    );

    let mut total_passed = 0usize;
    let mut total_tests = 0usize;
    let mut fully_passing = 0usize;
    let mut fully_failing = 0usize;

    for path in &entries {
        let Some(r) = run_fixture(path) else {
            continue;
        };
        print_result(&r);
        total_passed += r.passed;
        total_tests += r.total;
        if r.passed == r.total {
            fully_passing += 1;
        } else if r.passed == 0 {
            fully_failing += 1;
        }
    }

    println!();
    let rate = if total_tests > 0 {
        (total_passed as f64 / total_tests as f64) * 100.0
    } else {
        0.0
    };
    println!(
        "BASELINE TOTAL: {}/{} ({:.2}%)",
        total_passed, total_tests, rate
    );
    println!("  fully passing: {} / {}", fully_passing, entries.len());
    println!("  fully failing: {} / {}", fully_failing, entries.len());
}

/// Tiny smoke test for harness wiring: NOP is the simplest opcode in
/// the corpus and should pass on the type-alias 68020 today. If this
/// fails, the harness itself is broken (deserialiser, prefetch setup,
/// bus service); don't chase the bigger sweep.
#[test]
#[ignore]
fn harte_nop_smoke() {
    let root = fixture_root();
    let path = root.join("NOP.msgpack");
    if !path.exists() {
        eprintln!("Skipping: {} not found", path.display());
        return;
    }

    let r = run_fixture(&path).expect("NOP fixture loads");
    println!();
    print_result(&r);
    assert!(r.total > 0, "NOP fixture should have tests");
    assert_eq!(
        r.passed, r.total,
        "NOP should pass on Cpu68000-aliased Cpu68020 — harness wiring is broken if it doesn't",
    );
}
