/// Tom Harte Z80 single-step test harness.
///
/// Runs ~1.6M per-instruction tests that verify the Z80 produces correct
/// register state and memory changes for every opcode.
///
/// Run with: cargo test -p zilog-z80 --test single_step_tests -- --ignored --nocapture
mod support;

use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use support::find_tom_harte_z80_dir;
use zilog_z80::Z80;

#[derive(Deserialize)]
struct TestCase {
    name: String,
    initial: State,
    #[serde(rename = "final")]
    final_state: State,
    #[allow(dead_code)]
    cycles: Vec<serde_json::Value>, // We don't verify per-cycle bus events yet
}

#[derive(Deserialize)]
struct State {
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
    #[serde(default)]
    #[allow(dead_code)]
    ei: u8,
    wz: u16,
    ix: u16,
    iy: u16,
    af_: u16,
    bc_: u16,
    de_: u16,
    hl_: u16,
    im: u8,
    #[allow(dead_code)]
    p: u8,
    #[serde(default)]
    q: Option<u8>,
    iff1: u8,
    iff2: u8,
    ram: Vec<(u16, u8)>,
}

fn setup_z80(z80: &mut Z80, state: &State) {
    z80.regs.af = ((state.a as u16) << 8) | state.f as u16;
    z80.regs.bc = ((state.b as u16) << 8) | state.c as u16;
    z80.regs.de = ((state.d as u16) << 8) | state.e as u16;
    z80.regs.hl = ((state.h as u16) << 8) | state.l as u16;
    z80.regs.af_alt = state.af_;
    z80.regs.bc_alt = state.bc_;
    z80.regs.de_alt = state.de_;
    z80.regs.hl_alt = state.hl_;
    z80.regs.ix = state.ix;
    z80.regs.iy = state.iy;
    z80.regs.sp = state.sp;
    z80.regs.pc = state.pc;
    z80.regs.i = state.i;
    z80.regs.r = state.r;
    z80.regs.wz = state.wz;
    z80.regs.im = state.im;
    z80.regs.iff1 = state.iff1 != 0;
    z80.regs.iff2 = state.iff2 != 0;
    // Q register (if present in initial state)
    z80.regs.q = state.q.unwrap_or(0);
}

/// Per-opcode label allowlist for accepted Tom Harte disagreements.
///
/// Per `decisions/spectrum-test-oracle-priority.md`, Spectrum-validated
/// oracles (FUSE + Patrik Rak's z80memptr) outrank Tom Harte for
/// Spectrum work. The four block-I/O repeating instructions —
/// `INIR (ED B2)`, `OTIR (ED B3)`, `INDR (ED BA)`, `OTDR (ED BB)` —
/// have a WZ value at mid-repeat that FUSE expects to remain at
/// `BC ± 1` (the value set during the IN/OUT portion) but Tom Harte's
/// pre-2026 vectors recorded as `PC + 1` (a stale "we'll re-execute"
/// marker we used to set in the repeat handler). Both oracles can't
/// be right; we satisfy the Spectrum-priority side and document the
/// known WZ-only disagreements here.
const ACCEPTED_TOM_HARTE_DISAGREEMENTS: &[(&str, &[&str])] = &[
    ("ed b2", &["WZ"]),
    ("ed b3", &["WZ"]),
    ("ed ba", &["WZ"]),
    ("ed bb", &["WZ"]),
];

fn accepted_labels_for(opcode_stem: &str) -> &'static [&'static str] {
    ACCEPTED_TOM_HARTE_DISAGREEMENTS
        .iter()
        .find_map(|(stem, labels)| {
            if *stem == opcode_stem.to_lowercase() {
                Some(*labels)
            } else {
                None
            }
        })
        .unwrap_or(&[])
}

/// True if every reported error's label is in the per-opcode allowlist.
fn errors_within_allowlist(errors: &[String], allowed: &[&str]) -> bool {
    !errors.is_empty()
        && errors.iter().all(|err| {
            let label = err.split(':').next().unwrap_or("");
            allowed.contains(&label)
        })
}

fn check_z80(z80: &Z80, expected: &State, mem: &[u8; 65536]) -> Vec<String> {
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

    check!(
        "AF",
        z80.regs.af,
        ((expected.a as u16) << 8) | expected.f as u16
    );
    check!(
        "BC",
        z80.regs.bc,
        ((expected.b as u16) << 8) | expected.c as u16
    );
    check!(
        "DE",
        z80.regs.de,
        ((expected.d as u16) << 8) | expected.e as u16
    );
    check!(
        "HL",
        z80.regs.hl,
        ((expected.h as u16) << 8) | expected.l as u16
    );
    check!("AF'", z80.regs.af_alt, expected.af_);
    check!("BC'", z80.regs.bc_alt, expected.bc_);
    check!("DE'", z80.regs.de_alt, expected.de_);
    check!("HL'", z80.regs.hl_alt, expected.hl_);
    check!("IX", z80.regs.ix, expected.ix);
    check!("IY", z80.regs.iy, expected.iy);
    check!("SP", z80.regs.sp, expected.sp);
    check!("PC", z80.regs.pc, expected.pc);
    check!("WZ", z80.regs.wz, expected.wz);
    check!("I", z80.regs.i as u16, expected.i as u16);
    check!("R", z80.regs.r as u16, expected.r as u16);
    check!("IFF1", z80.regs.iff1 as u16, expected.iff1 as u16);
    check!("IFF2", z80.regs.iff2 as u16, expected.iff2 as u16);
    check!("IM", z80.regs.im as u16, expected.im as u16);

    // Check Q register if expected value is present
    if let Some(expected_q) = expected.q {
        check!("Q", z80.regs.q as u16, expected_q as u16);
    }

    // Check memory
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

/// Run a single test case: set up initial state, run one instruction, compare.
fn run_test(test: &TestCase) -> Vec<String> {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];
    // Separate I/O data map for port reads that differ from memory content.
    // Built from cycle data where signals contain 'i' (I/O read).
    let mut io_data: HashMap<u16, u8> = HashMap::new();
    for (i, cycle) in test.cycles.iter().enumerate() {
        if let Some(signals) = cycle.get(2).and_then(|v| v.as_str())
            && signals.contains('i') && signals.contains('r')
            // The data for this I/O read appears on the NEXT cycle.
            && let Some(next) = test.cycles.get(i + 1)
            && let (Some(addr), Some(data)) = (
                next.get(0).and_then(|v| v.as_u64()),
                next.get(1).and_then(|v| v.as_u64()),
            )
        {
            io_data.insert(addr as u16, data as u8);
        }
    }

    // Set up initial memory from the ram array
    for &(addr, val) in &test.initial.ram {
        mem[addr as usize] = val;
    }

    // Pre-populate memory: first pass from cycles that show data being READ.
    // We identify reads by looking at cycles where data appears and the
    // next cycle with the same address doesn't have a write signal.
    // Simpler approach: populate from all cycles with non-null data that
    // are NOT writes (signals contain 'w').
    for cycle in &test.cycles {
        if let (Some(addr), Some(data), Some(signals)) = (
            cycle.get(0).and_then(|v| v.as_u64()),
            cycle.get(1).and_then(|v| v.as_u64()),
            cycle.get(2).and_then(|v| v.as_str()),
        ) && !signals.contains('w')
        {
            mem[addr as usize] = data as u8;
        }
    }

    // Re-apply initial.ram (takes priority)
    for &(addr, val) in &test.initial.ram {
        mem[addr as usize] = val;
    }

    // Set up initial Z80 state
    setup_z80(&mut z80, &test.initial);

    // The cycles array has one entry per T-state of bus activity.
    // Our Z80 operates in half-cycles (2 HCs per T-state).
    let expected_hc = test.cycles.len() as u32 * 2;

    // Run for the expected number of half-cycles.
    for _ in 0..expected_hc {
        z80.tick();

        // Handle bus transactions
        if z80.mreq && z80.rd {
            z80.data_in = mem[z80.addr as usize];
        } else if z80.mreq && z80.wr {
            mem[z80.addr as usize] = z80.data;
        } else if z80.iorq && z80.rd && !z80.m1 {
            // Use I/O data map if available (handles port/memory address collisions)
            z80.data_in = io_data
                .get(&z80.addr)
                .copied()
                .unwrap_or(mem[z80.addr as usize]);
        } else if z80.iorq && z80.wr {
            // I/O write — no action in test harness
        }
    }

    // Compare final state
    check_z80(&z80, &test.final_state, &mem)
}

/// Run all tests in a single JSON file.
fn run_opcode_tests(path: &Path) -> (usize, usize, Vec<String>) {
    let data = std::fs::read_to_string(path).expect("Failed to read test file");
    let tests: Vec<TestCase> = serde_json::from_str(&data).expect("Failed to parse JSON");

    let opcode_stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let allowed = accepted_labels_for(&opcode_stem);

    let mut pass = 0;
    let mut fail = 0;
    let mut first_failures = Vec::new();

    for test in &tests {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_test(test)));
        match result {
            Ok(errors) if errors.is_empty() => {
                pass += 1;
            }
            Ok(errors) if errors_within_allowlist(&errors, allowed) => {
                // Disagreement is confined to per-opcode allowlist —
                // count as a (documented) pass.
                pass += 1;
            }
            Ok(errors) => {
                fail += 1;
                if first_failures.len() < 3 {
                    first_failures.push(format!("FAIL {}: {}", test.name, errors.join(", ")));
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

#[test]
#[ignore = "requires local Tom Harte Z80 corpus and runs for minutes"]
fn run_all() {
    // Fail rather than skip. This is a declared accuracy gate, and it
    // is `#[ignore]`d — reaching it means someone asked for it by name.
    // Returning early on a missing corpus still reports `test result:
    // ok`, which is indistinguishable from 1,604,000 passing vectors in
    // a log or a CI summary; a baseline was very nearly recorded as
    // "Tom Harte 100%" from a run that executed nothing. Same principle
    // as the catalogue's routing-version constants: an absent or stale
    // oracle must be loud, not quietly green.
    let test_path = find_tom_harte_z80_dir().unwrap_or_else(|message| panic!("{message}"));

    let read_dir = match std::fs::read_dir(&test_path) {
        Ok(read_dir) => read_dir,
        Err(error) => panic!(
            "failed to read test directory {}: {error}",
            test_path.display()
        ),
    };

    let mut entries: Vec<_> = read_dir
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    let mut total_pass = 0usize;
    let mut total_fail = 0usize;
    let mut failed_opcodes: HashMap<String, Vec<String>> = HashMap::new();

    for entry in &entries {
        let path = entry.path();
        let Some(stem) = path.file_stem() else {
            panic!("missing file stem for {}", path.display());
        };
        let Some(name) = stem.to_str() else {
            panic!("non-utf8 file stem for {}", path.display());
        };
        let name = name.to_string();
        let (pass, fail, failures) = run_opcode_tests(&path);
        total_pass += pass;
        total_fail += fail;

        if fail > 0 {
            println!("  {} — {}/{} pass ({} fail)", name, pass, pass + fail, fail);
            for f in &failures {
                println!("    {}", f);
            }
            failed_opcodes.insert(name, failures);
        }
    }

    let total = total_pass + total_fail;
    let pct = if total > 0 {
        (total_pass as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    println!();
    println!("=== Tom Harte Z80 Tests ===");
    println!("Total: {}/{} pass ({:.2}%)", total_pass, total, pct);
    println!("Failed opcodes: {}", failed_opcodes.len());

    if total_fail > 0 {
        println!("\nFailed opcodes:");
        let mut keys: Vec<_> = failed_opcodes.keys().collect();
        keys.sort();
        for key in keys {
            println!("  {}", key);
        }
    }

    assert_eq!(
        total_fail, 0,
        "Expected 100% pass rate, got {}/{} ({:.2}%)",
        total_pass, total, pct
    );
}

/// Run tests for a single opcode (useful for debugging).
/// Example: cargo test -p zilog-z80 --test single_step_tests run_opcode_00 -- --ignored --nocapture
#[test]
#[ignore = "requires local Tom Harte Z80 corpus"]
fn run_opcode_00() {
    let path = match find_tom_harte_z80_dir() {
        Ok(dir) => dir.join("00.json"),
        Err(message) => {
            eprintln!("{message}");
            return;
        }
    };
    let (pass, fail, failures) = run_opcode_tests(&path);
    println!("00 (NOP): {}/{} pass", pass, pass + fail);
    for f in &failures {
        println!("  {}", f);
    }
    assert_eq!(fail, 0);
}
