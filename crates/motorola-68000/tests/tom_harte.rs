//! Tom Harte 680x0 single-step test harness.
//!
//! Runs the canonical Tom Harte fixture suite at
//! `~/Projects/Emu198x-archive/test-data/680x0/68000/v1/*.json.gz`
//! against the pin-level CPU core.
//!
//! Each fixture file contains ~8000 single-instruction tests for
//! one opcode variant. Each test gives full CPU + memory state
//! before and after execution. The harness sets up the CPU from
//! the `initial` block, ticks it until the next instruction starts,
//! and compares against the `final` block.
//!
//! Skipped under normal `cargo test`. Run explicitly with:
//!
//! ```sh
//! cargo test -p motorola-68000 --test tom_harte -- --ignored --nocapture
//! ```
//!
//! The default test (`harte_smoke`) runs a small curated subset of
//! opcodes that are load-bearing for the Amiga graphics.library
//! init code path we're investigating. The full sweep
//! (`harte_full_sweep`) runs all 125 opcode files.

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use serde_json::Value;

use motorola_68000::Cpu68000;
use motorola_68000::bus::{BusStatus, FunctionCode};
use motorola_68000::cpu::State;

// ─── Sparse memory ────────────────────────────────────────────────

/// Sparse byte memory for the 24-bit address space the 68000 sees.
///
/// Tom Harte fixtures only touch a handful of bytes per test, so
/// `HashMap<u32,u8>` is the obvious representation. Unset locations
/// read as zero; the fixture provides initial values for anything
/// the instruction reads.
struct SparseMem {
    bytes: HashMap<u32, u8>,
}

impl SparseMem {
    fn new() -> Self {
        Self {
            bytes: HashMap::new(),
        }
    }

    fn load_ram(&mut self, ram: &[Value]) {
        for entry in ram {
            let pair = entry.as_array().expect("ram entry is array");
            let addr = pair[0].as_u64().expect("ram addr") as u32;
            let value = pair[1].as_u64().expect("ram value") as u8;
            self.bytes.insert(addr & 0xFF_FFFF, value);
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

fn fixture_root() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME set");
    PathBuf::from(home).join("Projects/Emu198x-archive/test-data/680x0/68000/v1")
}

fn load_fixture(path: &Path) -> Option<Vec<Value>> {
    let file = File::open(path).ok()?;
    let gz = GzDecoder::new(file);
    let parsed: Value = serde_json::from_reader(gz).ok()?;
    Some(parsed.as_array()?.clone())
}

// ─── CPU state setup from fixture ─────────────────────────────────

fn apply_initial(cpu: &mut Cpu68000, mem: &mut SparseMem, initial: &Value) {
    let o = initial.as_object().expect("initial is object");

    for i in 0..8 {
        let k = format!("d{i}");
        cpu.regs.d[i] = o[&k].as_u64().expect("dN") as u32;
    }
    for i in 0..7 {
        let k = format!("a{i}");
        cpu.regs.a[i] = o[&k].as_u64().expect("aN") as u32;
    }
    cpu.regs.usp = o["usp"].as_u64().expect("usp") as u32;
    cpu.regs.ssp = o["ssp"].as_u64().expect("ssp") as u32;
    let pc = o["pc"].as_u64().expect("pc") as u32;
    cpu.regs.sr = o["sr"].as_u64().expect("sr") as u16;

    let pq = o["prefetch"].as_array().expect("prefetch array");
    let pf0 = pq[0].as_u64().expect("prefetch0") as u16;
    let pf1 = pq[1].as_u64().expect("prefetch1") as u16;

    // Put both prefetch words into memory so FetchIRC can read them.
    mem.write_word(pc, pf0);
    mem.write_word(pc.wrapping_add(2), pf1);

    // Use the CPU's own setup_prefetch(), which correctly initialises
    // all pipeline registers (IR, IRC, irc_addr, next_fetch_addr,
    // instr_start_pc) and directly queues Execute — bypassing the
    // PromoteIRC step and getting the prefetch state exactly right.
    //
    // setup_prefetch expects regs.pc to already point past both the
    // opcode word and the IRC word: pc = fixture_pc + 4.
    cpu.regs.pc = pc.wrapping_add(4);
    cpu.setup_prefetch(pf0, pf1);
}

// ─── Bus service (same pattern as pin_level.rs) ────────────────────

fn service_bus(cpu: &mut Cpu68000, mem: &mut SparseMem) {
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

// ─── Run exactly one instruction ───────────────────────────────────

/// Run the CPU until the fixture instruction has completed.
///
/// Setup leaves `cpu.irc = prefetch[0]` and `instr_start_pc =
/// fixture PC`. The first `PromoteIRC` rotates prefetch[0] into IR
/// and sets `instr_start_pc = irc_addr = fixture PC` (so the first
/// promote is a no-op from this harness's viewpoint). The
/// instruction then executes (FetchIRC + Execute micro-ops). When
/// it completes, `start_next_instruction` queues another
/// PromoteIRC, and the NEXT promote moves `instr_start_pc` to the
/// NEW address (either fixture PC+2 for simple instructions, or
/// wherever PC landed for jumps).
///
/// So "instruction complete" = `instr_start_pc` has changed AWAY
/// from `fixture PC`. We count ticks with a wide bound to allow
/// even the longest instructions (TRAP, MOVEM, etc.) to finish.
fn run_one_instruction(cpu: &mut Cpu68000, mem: &mut SparseMem, fixture_pc: u32) -> bool {
    for _ in 0..400 {
        service_bus(cpu, mem);
        cpu.tick();
        if cpu.instr_start_pc != fixture_pc {
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

fn compare_final(cpu: &Cpu68000, mem: &SparseMem, final_state: &Value) -> Vec<Mismatch> {
    let mut v = Vec::new();
    let o = final_state.as_object().expect("final is object");

    for i in 0..8 {
        let k = format!("d{i}");
        let expected = o[&k].as_u64().unwrap() as u32;
        if cpu.regs.d[i] != expected {
            v.push(Mismatch {
                field: k,
                expected: format!("${expected:08X}"),
                actual: format!("${:08X}", cpu.regs.d[i]),
            });
        }
    }
    for i in 0..7 {
        let k = format!("a{i}");
        let expected = o[&k].as_u64().unwrap() as u32;
        if cpu.regs.a[i] != expected {
            v.push(Mismatch {
                field: k,
                expected: format!("${expected:08X}"),
                actual: format!("${:08X}", cpu.regs.a[i]),
            });
        }
    }

    let expected_usp = o["usp"].as_u64().unwrap() as u32;
    if cpu.regs.usp != expected_usp {
        v.push(Mismatch {
            field: "usp".into(),
            expected: format!("${expected_usp:08X}"),
            actual: format!("${:08X}", cpu.regs.usp),
        });
    }
    let expected_ssp = o["ssp"].as_u64().unwrap() as u32;
    if cpu.regs.ssp != expected_ssp {
        v.push(Mismatch {
            field: "ssp".into(),
            expected: format!("${expected_ssp:08X}"),
            actual: format!("${:08X}", cpu.regs.ssp),
        });
    }
    // Tom Harte's "final pc" is the address of the NEXT instruction
    // (= where the prefetched opcode came from). In this CPU's prefetch
    // model that's `instr_start_pc` after the current instruction's
    // completion, NOT `regs.pc` (which is instr_start_pc + 2).
    let expected_pc = o["pc"].as_u64().unwrap() as u32;
    if cpu.instr_start_pc != expected_pc {
        v.push(Mismatch {
            field: "pc".into(),
            expected: format!("${expected_pc:08X}"),
            actual: format!("${:08X}", cpu.instr_start_pc),
        });
    }
    let expected_sr = o["sr"].as_u64().unwrap() as u16;
    if cpu.regs.sr != expected_sr {
        v.push(Mismatch {
            field: "sr".into(),
            expected: format!("${expected_sr:04X}"),
            actual: format!("${:04X}", cpu.regs.sr),
        });
    }

    // Memory.
    for entry in o["ram"].as_array().unwrap() {
        let pair = entry.as_array().unwrap();
        let addr = pair[0].as_u64().unwrap() as u32;
        let expected = pair[1].as_u64().unwrap() as u8;
        let actual = mem.read_byte(addr);
        if actual != expected {
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
    let tests = load_fixture(path)?;
    let name = path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .trim_end_matches(".json.gz")
        .to_string();

    let mut passed = 0;
    let mut first_fail: Option<(String, Vec<Mismatch>)> = None;

    for test in &tests {
        let t = test.as_object().unwrap();
        let case_name = t["name"].as_str().unwrap_or("?").to_string();
        let initial = &t["initial"];
        let final_state = &t["final"];

        let mut cpu = Cpu68000::new();
        let mut mem = SparseMem::new();
        mem.load_ram(initial["ram"].as_array().unwrap());
        apply_initial(&mut cpu, &mut mem, initial);
        let fixture_pc = initial["pc"].as_u64().unwrap() as u32;

        let ran = run_one_instruction(&mut cpu, &mut mem, fixture_pc);
        if !ran {
            if first_fail.is_none() {
                first_fail = Some((
                    case_name,
                    vec![Mismatch {
                        field: "run".into(),
                        expected: "1 instruction".into(),
                        actual: "timeout".into(),
                    }],
                ));
            }
            continue;
        }

        let mismatches = compare_final(&cpu, &mem, final_state);
        if mismatches.is_empty() {
            passed += 1;
        } else if first_fail.is_none() {
            first_fail = Some((case_name, mismatches));
        }
    }

    Some(FixtureResult {
        name,
        total: tests.len(),
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
        "  {:<24} {:>5}/{:<5} ({:>5.1}%)",
        r.name, r.passed, r.total, rate
    );
    if let Some((case, mismatches)) = &r.first_fail {
        println!("    first fail: {case}");
        for m in mismatches.iter().take(6) {
            println!(
                "      {:<16} expected={:<12} actual={}",
                m.field, m.expected, m.actual
            );
        }
        if mismatches.len() > 6 {
            println!("      ... and {} more", mismatches.len() - 6);
        }
    }
}

// ─── Tests ─────────────────────────────────────────────────────────

/// Run a small curated set of opcodes that the Amiga graphics.library
/// init code path depends on heavily. This is the first filter —
/// if any of these fail the full sweep is redundant until fixed.
#[test]
#[ignore]
fn harte_smoke() {
    let root = fixture_root();
    if !root.exists() {
        eprintln!("Skipping: fixture dir not found at {}", root.display());
        return;
    }

    // Load-bearing opcodes for the graphics.library init path:
    //   MOVE.L/.W/.b (all addressing modes)
    //   LEA, PEA
    //   ADDQ, SUBQ
    //   AND, OR, ANDI-to-CCR/SR, ORI
    //   BTST, BCLR, BSET
    //   Bcc, DBcc, BSR, BRA, JMP, JSR, RTS
    //   CLR, CMPI, MOVEQ
    //   TST
    //   MOVEM
    let smoke = [
        "MOVE.b", "MOVE.w", "MOVE.l", "MOVE.q", "MOVEA.w", "MOVEA.l", "MOVEM.w", "MOVEM.l", "LEA",
        "PEA", "ADD.w", "ADD.l", "SUB.w", "SUB.l", "AND.w", "AND.l", "OR.w", "OR.l", "CMP.w",
        "CMP.l", "BTST", "BCLR", "BSET", "BCHG", "Bcc", "BSR", "DBcc", "JMP", "JSR", "RTS", "RTE",
        "CLR.w", "CLR.l", "TST.w", "TST.l", "EXT.w", "EXT.l", "SWAP", "LSL.w", "LSR.w", "ASL.w",
        "ASR.w", "NOP",
    ];

    println!();
    println!("Tom Harte 680x0 smoke test ({} opcodes):", smoke.len());
    let mut total_passed = 0;
    let mut total_tests = 0;
    let mut fail_count = 0;

    for name in smoke {
        let path = root.join(format!("{name}.json.gz"));
        let Some(r) = run_fixture(&path) else {
            println!("  {name:<24} (fixture not found)");
            continue;
        };
        print_result(&r);
        total_passed += r.passed;
        total_tests += r.total;
        if r.passed != r.total {
            fail_count += 1;
        }
    }

    println!();
    let rate = (total_passed as f64 / total_tests as f64) * 100.0;
    println!(
        "SMOKE TOTAL: {}/{} ({:.1}%) — {} opcodes with failures",
        total_passed, total_tests, rate, fail_count
    );
}

/// Run the full 125-opcode fixture sweep. Slow — takes several
/// minutes. Report per-opcode pass rates.
#[test]
#[ignore]
fn harte_full_sweep() {
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
                .map(|n| n.to_string_lossy().ends_with(".json.gz"))
                .unwrap_or(false)
        })
        .collect();
    entries.sort();

    println!();
    println!("Tom Harte 680x0 full sweep ({} fixtures):", entries.len());

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
    let rate = (total_passed as f64 / total_tests as f64) * 100.0;
    println!(
        "FULL TOTAL: {}/{} ({:.2}%)",
        total_passed, total_tests, rate
    );
    println!("  fully passing: {} / {}", fully_passing, entries.len());
    println!("  fully failing: {} / {}", fully_failing, entries.len());
}
