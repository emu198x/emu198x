/// Tom Harte Z80 single-step test harness.
///
/// Runs ~1.6M per-instruction tests that verify the Z80 produces correct
/// register state and memory changes for every opcode.
///
/// Run with: cargo test -p zilog-z80 --test single_step_tests -- --ignored --nocapture
mod support;

use emu198x_zilog_z80::Z80;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use support::find_tom_harte_z80_dir;

#[derive(Deserialize)]
struct TestCase {
    name: String,
    initial: State,
    #[serde(rename = "final")]
    final_state: State,
    cycles: Vec<Cycle>, // Used for stimulus/budget, not bus-trace comparison
}

// Keep nullable bus values, but reject malformed rows and lossy integers.
#[derive(Deserialize)]
struct Cycle(Option<u16>, Option<u8>, String);

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
    assert!(
        !test.cycles.is_empty(),
        "{}: no execution cycles",
        test.name
    );
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];
    // Separate I/O data map for port reads that differ from memory content.
    // Built from cycle data where signals contain 'i' (I/O read).
    let mut io_data: HashMap<u16, u8> = HashMap::new();
    for (i, cycle) in test.cycles.iter().enumerate() {
        if cycle.2.contains('i') && cycle.2.contains('r')
            // The data for this I/O read appears on the NEXT cycle.
            && let Some(next) = test.cycles.get(i + 1)
            && let (Some(addr), Some(data)) = (next.0, next.1)
        {
            io_data.insert(addr, data);
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
        if let (Some(addr), Some(data)) = (cycle.0, cycle.1)
            && !cycle.2.contains('w')
        {
            mem[addr as usize] = data;
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

fn parse_opcode_tests(data: &str, path: &Path) -> Vec<TestCase> {
    let tests: Vec<TestCase> = serde_json::from_str(data).expect("Failed to parse JSON");
    assert!(!tests.is_empty(), "no test cases in {}", path.display());
    tests
}

#[test]
#[should_panic(expected = "no test cases in empty.json")]
fn rejects_empty_opcode_file() {
    parse_opcode_tests("[]", Path::new("empty.json"));
}

#[test]
fn rejects_empty_corpus_directory() {
    let root = std::env::temp_dir().join(format!("emu198x-empty-harte-{}", std::process::id()));
    std::fs::create_dir(&root).expect("create empty corpus directory");
    let result = std::panic::catch_unwind(|| run_all_from_dir(&root));
    std::fs::remove_dir(&root).expect("remove empty corpus directory");
    let panic = result.expect_err("empty corpus must fail");
    let message = panic.downcast_ref::<String>().expect("diagnostic string");
    assert!(message.contains("no opcode JSON files"), "{message}");
}

/// Run all tests in a single JSON file.
fn run_opcode_tests(path: &Path) -> (usize, usize, usize, Vec<String>) {
    let data = std::fs::read_to_string(path).expect("Failed to read test file");
    let tests = parse_opcode_tests(&data, path);

    let mut pass = 0;
    let accepted = 0; // No accepted differences: every compared value is strict.
    let mut fail = 0;
    let mut first_failures = Vec::new();

    for test in &tests {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_test(test)));
        match result {
            Ok(errors) if errors.is_empty() => {
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

    (pass, accepted, fail, first_failures)
}

#[test]
#[ignore = "FIXTURE: requires local Tom Harte Z80 corpus and runs for minutes"]
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

    run_all_from_dir(&test_path);
}

fn run_all_from_dir(test_path: &Path) {
    let read_dir = match std::fs::read_dir(test_path) {
        Ok(read_dir) => read_dir,
        Err(error) => panic!(
            "failed to read test directory {}: {error}",
            test_path.display()
        ),
    };

    let mut entries: Vec<_> = read_dir
        .map(|entry| entry.expect("failed to read corpus directory entry"))
        .filter(|entry| entry.path().extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();
    entries.sort_by_key(|e| e.file_name());
    assert!(
        !entries.is_empty(),
        "no opcode JSON files in {}",
        test_path.display()
    );

    let mut total_pass = 0usize;
    let mut total_accepted = 0usize;
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
        let (pass, accepted, fail, failures) = run_opcode_tests(&path);
        total_pass += pass;
        total_accepted += accepted;
        total_fail += fail;

        if fail > 0 || accepted > 0 {
            println!("  {name} — {pass} exact, {accepted} accepted, {fail} unexpected");
            for f in &failures {
                println!("    {}", f);
            }
            if fail > 0 {
                failed_opcodes.insert(name, failures);
            }
        }
    }

    let total = total_pass + total_accepted + total_fail;
    println!();
    println!("=== Tom Harte Z80 Tests ===");
    println!(
        "Total: {total} executed, {total_pass} exact, {total_accepted} accepted, {total_fail} unexpected"
    );
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
        "Tom Harte reported {total_fail} unexpected failures out of {total} cases"
    );
}

/// Run tests for a single opcode (useful for debugging).
/// Example: cargo test -p zilog-z80 --test single_step_tests run_opcode_00 -- --ignored --nocapture
#[test]
#[ignore = "FIXTURE: requires local Tom Harte Z80 corpus"]
fn run_opcode_00() {
    let path = find_tom_harte_z80_dir()
        .unwrap_or_else(|message| panic!("{message}"))
        .join("00.json");
    let (pass, accepted, fail, failures) = run_opcode_tests(&path);
    println!("00 (NOP): {pass} exact, {accepted} accepted, {fail} unexpected");
    for f in &failures {
        println!("  {}", f);
    }
    assert_eq!(fail, 0);
}

fn synthetic_otir_case() -> TestCase {
    let initial = serde_json::json!({
        "pc": 0, "sp": 0, "a": 0, "b": 3, "c": 224, "d": 0, "e": 0,
        "f": 0, "h": 0, "l": 0, "i": 0, "r": 0, "wz": 0,
        "ix": 0, "iy": 0, "af_": 0, "bc_": 0, "de_": 0, "hl_": 0,
        "im": 0, "p": 0, "iff1": 0, "iff2": 0, "ram": []
    });
    let mut final_state = initial.clone();
    final_state["b"] = 2.into();
    final_state["wz"] = 1.into();
    serde_json::from_value(serde_json::json!({
        "name": "synthetic repeating OTIR", "initial": initial, "final": final_state,
        "cycles": vec![serde_json::json!([0, null, "----"]); 21]
    }))
    .expect("synthetic fixture")
}

#[test]
#[should_panic(expected = "no execution cycles")]
fn rejects_case_that_executes_nothing() {
    let mut test = synthetic_otir_case();
    test.final_state.b = test.initial.b;
    test.final_state.wz = test.initial.wz;
    test.cycles.clear();
    // All compared state is unchanged: without the guard this passes.
    assert!(run_test(&test).is_empty());
}

#[test]
fn cycle_rows_reject_malformed_or_lossy_data() {
    for data in [
        "null",
        "[]",
        "[0, null]",
        "[0, null, 42]",
        "[0, null, null]",
        "[0, null, \"----\", 1]",
        "[65536, null, \"----\"]",
        "[-1, null, \"----\"]",
        "[0, 256, \"----\"]",
        "[0, -1, \"----\"]",
        "[0, 1.5, \"----\"]",
    ] {
        assert!(
            serde_json::from_str::<Cycle>(data).is_err(),
            "accepted malformed cycle: {data}"
        );
    }
    let cycle: Cycle = serde_json::from_str("[65535, 255, \"r-m-\"]").expect("maximum bus values");
    assert_eq!((cycle.0, cycle.1), (Some(65535), Some(255)));
    let idle: Cycle = serde_json::from_str("[null, null, \"----\"]").expect("nullable idle bus");
    assert_eq!((idle.0, idle.1), (None, None));
}

#[test]
fn explicit_opcode_run_requires_fixtures() {
    let missing = std::env::temp_dir().join(format!(
        "emu198x-missing-opcode-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos()
    ));
    assert!(!missing.exists());
    let output = std::process::Command::new(std::env::current_exe().expect("test executable"))
        .args(["--ignored", "--exact", "run_opcode_00", "--nocapture"])
        .env("EMU198X_Z80_TOM_HARTE_DIR", &missing)
        .env_remove("EMU198X_STRICT_FIXTURES")
        .output()
        .expect("run explicit opcode test");
    assert!(
        !output.status.success(),
        "explicit test silently skipped missing fixtures"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("EMU198X_Z80_TOM_HARTE_DIR"), "{stderr}");
    assert!(stderr.contains(&missing.display().to_string()), "{stderr}");
}
