//! Adam Tennant SM83 single-step test harness.
//!
//! Runs the per-opcode JSON corpus from
//! <https://github.com/adtennant/sm83-test-data> against the
//! `sharp-lr35902` crate. The corpus is Tom Harte-format JSON: 100
//! tests per opcode, with `initial`/`final` register snapshots and a
//! `cycles` array describing the bus activity per m-cycle.
//!
//! HALT/STOP/EI/DI are intentionally omitted by the corpus (their
//! semantics aren't single-step-test friendly); the crate's own unit
//! tests cover them.
//!
//! Run one smoke opcode:
//!
//! ```sh
//! cargo test -p sharp-lr35902 --test single_step_tests run_opcode_00 \
//!   -- --ignored --nocapture
//! ```
//!
//! Run a single opcode by hex:
//!
//! ```sh
//! EMU198X_SM83_OPCODE=cd cargo test -p sharp-lr35902 \
//!   --test single_step_tests run_named_opcode -- --ignored --nocapture
//! ```
//!
//! Run the full corpus:
//!
//! ```sh
//! cargo test -p sharp-lr35902 --test single_step_tests run_all \
//!   -- --ignored --nocapture
//! ```
//!
//! The `cb` sub-table (`cb.json`) holds 25,600 tests covering every
//! `CB xx` permutation; it's run as a separate test (`run_cb`).
//!
//! ## Pipelined-model adapter
//!
//! The corpus assumes a *decode-execute-prefetch* loop: each test's
//! `initial.pc` already points one past the opcode byte (the decode
//! happened "before" the test) and the final cycle in the `cycles`
//! array is the prefetch of the next instruction (which both reads
//! the next opcode and increments PC by 1).
//!
//! This crate's CPU is pin-level pipelined the other way: the opcode
//! fetched by the previous instruction's prefetch lives in `data_in`,
//! and `pc` is advanced when the next boundary tick consumes it. To
//! map between the two:
//!
//! 1. Set the CPU PC to `initial.pc - 1` (the opcode address) and
//!    prime `data_in`/pins for the opcode fetch the corpus assumes
//!    has just happened.
//! 2. Run one warmup tick: this consumes the opcode at the boundary,
//!    advances PC to `initial.pc`, and lets the dispatch arm schedule
//!    the first follow-up bus op (matching `cycles[0]`).
//! 3. For each cycle, verify pin state, route data_in / capture
//!    writes, then run the next tick — except for the *final* cycle
//!    (the prefetch), which is only pin-checked. The data fetched
//!    isn't consumed because that would advance into the next
//!    instruction's m-cycle 1 arm and could mutate registers.
//! 4. Synthesise the prefetch's PC++ (one line of `cpu.pc += 1`) so
//!    `final.pc` matches.

mod support;

use std::path::Path;

use serde::Deserialize;
use sharp_lr35902::Sm83;
use support::find_sm83_tennant_dir;

const OPCODE_ENV: &str = "EMU198X_SM83_OPCODE";

#[derive(Deserialize)]
struct TestCase {
    name: String,
    initial: State,
    #[serde(rename = "final")]
    final_state: State,
    /// `null` entries represent bus-idle cycles. Non-null entries are
    /// `[address, value, "read"|"write"]`.
    cycles: Vec<Option<Cycle>>,
}

#[derive(Deserialize)]
struct State {
    a: u8,
    b: u8,
    c: u8,
    d: u8,
    e: u8,
    f: u8,
    h: u8,
    l: u8,
    pc: u16,
    sp: u16,
    ram: Vec<(u16, u8)>,
}

#[derive(Deserialize)]
struct Cycle(u16, u8, String);

impl Cycle {
    fn addr(&self) -> u16 {
        self.0
    }
    fn value(&self) -> u8 {
        self.1
    }
    fn is_write(&self) -> bool {
        self.2 == "write"
    }
}

fn build_ram(initial_ram: &[(u16, u8)]) -> Vec<u8> {
    let mut ram = vec![0u8; 0x10000];
    for &(addr, val) in initial_ram {
        ram[addr as usize] = val;
    }
    ram
}

fn run_test(test: &TestCase) -> Vec<String> {
    let mut errors = Vec::new();
    let mut ram = build_ram(&test.initial.ram);

    // Locate the opcode that the corpus's "decode" step consumed:
    // it's the byte one before initial.pc (or initial.pc - 0 if the
    // opcode lives at the start of a 64K wrap, which is rare but
    // legal). The corpus puts it in initial.ram explicitly, so we can
    // look it up.
    let opcode_addr = test.initial.pc.wrapping_sub(1);
    let opcode = ram[opcode_addr as usize];

    let mut cpu = Sm83::new();
    cpu.a = test.initial.a;
    cpu.b = test.initial.b;
    cpu.c = test.initial.c;
    cpu.d = test.initial.d;
    cpu.e = test.initial.e;
    cpu.f = test.initial.f;
    cpu.h = test.initial.h;
    cpu.l = test.initial.l;
    cpu.sp = test.initial.sp;
    // Pretend we're the boundary tick: place PC at the opcode byte,
    // pre-load data_in with the opcode, prime pins for the just-
    // happened opcode fetch, and let the warmup tick run the boundary
    // logic.
    cpu.pc = opcode_addr;
    cpu.data_in = opcode;
    cpu.addr = opcode_addr;
    cpu.rd = true;
    cpu.wr = false;
    cpu.mreq = true;
    cpu.m_cycle = 0;
    cpu.dispatching = false;
    cpu.ime = false;
    cpu.ime_pending = false;
    cpu.irq_pending = 0;

    cpu.tick(); // Warmup: consume opcode, dispatch m_cycle=1 arm,
    // schedule the first follow-up bus op (cycles[0]).

    for (i, cycle) in test.cycles.iter().enumerate() {
        match cycle {
            None => {
                if cpu.mreq {
                    errors.push(format!(
                        "cycle {i}: expected internal, CPU is driving \
                         {} at ${:04X}",
                        if cpu.rd { "read" } else { "write" },
                        cpu.addr
                    ));
                }
            }
            Some(c) if c.is_write() => {
                if !cpu.mreq || !cpu.wr {
                    errors.push(format!(
                        "cycle {i}: expected write, CPU mreq={} wr={}",
                        cpu.mreq, cpu.wr
                    ));
                }
                if cpu.addr != c.addr() {
                    errors.push(format!(
                        "cycle {i}: write addr ${:04X} expected, got ${:04X}",
                        c.addr(),
                        cpu.addr
                    ));
                }
                if cpu.data != c.value() {
                    errors.push(format!(
                        "cycle {i}: write value ${:02X} expected, got ${:02X}",
                        c.value(),
                        cpu.data
                    ));
                }
                ram[cpu.addr as usize] = cpu.data;
            }
            Some(c) => {
                if !cpu.mreq || !cpu.rd {
                    errors.push(format!(
                        "cycle {i}: expected read, CPU mreq={} rd={}",
                        cpu.mreq, cpu.rd
                    ));
                }
                if cpu.addr != c.addr() {
                    errors.push(format!(
                        "cycle {i}: read addr ${:04X} expected, got ${:04X}",
                        c.addr(),
                        cpu.addr
                    ));
                }
                cpu.data_in = c.value();
            }
        }

        // The last cycle is the prefetch of the next instruction; we
        // only check its pin state but never run a tick that would
        // consume it (which would step into the next opcode's m-cycle
        // 1 arm and could mutate registers).
        if i + 1 < test.cycles.len() {
            cpu.tick();
        }
    }

    // The corpus's decode-execute-prefetch model treats the prefetch
    // m-cycle as also incrementing PC; our model defers that
    // increment to the next boundary tick. Synthesize it here so
    // final.pc lines up.
    cpu.pc = cpu.pc.wrapping_add(1);

    macro_rules! check {
        ($name:expr, $got:expr, $exp:expr) => {
            if $got != $exp {
                errors.push(format!(
                    "{}: got {:#06X}, expected {:#06X}",
                    $name, $got as u32, $exp as u32
                ));
            }
        };
    }
    check!("A", cpu.a, test.final_state.a);
    check!("B", cpu.b, test.final_state.b);
    check!("C", cpu.c, test.final_state.c);
    check!("D", cpu.d, test.final_state.d);
    check!("E", cpu.e, test.final_state.e);
    check!("F", cpu.f, test.final_state.f);
    check!("H", cpu.h, test.final_state.h);
    check!("L", cpu.l, test.final_state.l);
    check!("SP", cpu.sp, test.final_state.sp);
    check!("PC", cpu.pc, test.final_state.pc);

    for &(addr, val) in &test.final_state.ram {
        if ram[addr as usize] != val {
            errors.push(format!(
                "RAM[${addr:04X}]: got ${:02X}, expected ${val:02X}",
                ram[addr as usize]
            ));
        }
    }

    if !errors.is_empty() {
        errors.insert(0, format!("test '{}'", test.name));
    }
    errors
}

fn run_opcode_tests(path: &Path) -> (usize, usize, Vec<String>) {
    let data = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    let tests: Vec<TestCase> = serde_json::from_str(&data)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()));

    let mut pass = 0usize;
    let mut fail = 0usize;
    let mut first_failures = Vec::new();

    for test in &tests {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_test(test)));
        match result {
            Ok(errors) if errors.is_empty() => pass += 1,
            Ok(errors) => {
                fail += 1;
                if first_failures.len() < 3 {
                    first_failures.push(errors.join("\n  "));
                }
            }
            Err(_) => {
                fail += 1;
                if first_failures.len() < 3 {
                    first_failures.push(format!("PANIC '{}'", test.name));
                }
            }
        }
    }

    (pass, fail, first_failures)
}

fn opcode_path(root: &Path, opcode: u8) -> std::path::PathBuf {
    root.join(format!("{opcode:02x}.json"))
}

fn cb_path(root: &Path) -> std::path::PathBuf {
    root.join("cb.json")
}

fn named_opcode_from_env() -> Option<u8> {
    match std::env::var(OPCODE_ENV) {
        Ok(value) if !value.trim().is_empty() => Some(
            u8::from_str_radix(value.trim(), 16).unwrap_or_else(|error| {
                panic!("failed to parse {OPCODE_ENV}='{value}' as hex opcode: {error}")
            }),
        ),
        _ => None,
    }
}

#[test]
fn opcode_path_uses_lowercase_hex_names() {
    let path = opcode_path(Path::new("/tmp/sm83"), 0xCD);
    assert!(path.ends_with("cd.json"));
}

#[test]
#[ignore = "FIXTURE: requires local Adam Tennant SM83 corpus"]
fn run_opcode_00() {
    let root = find_sm83_tennant_dir().expect("SM83 corpus should exist");
    let (pass, fail, first_failures) = run_opcode_tests(&opcode_path(&root, 0x00));

    println!("opcode 00 (NOP): {pass} passed, {fail} failed");
    for failure in &first_failures {
        println!("{failure}");
    }
    assert_eq!(fail, 0, "opcode 00 reported {fail} failures");
}

#[test]
#[ignore = "FIXTURE: requires local Adam Tennant SM83 corpus"]
fn run_named_opcode() {
    let Some(opcode) = named_opcode_from_env() else {
        panic!("set {OPCODE_ENV} to one hex opcode, for example 'cd' or '3e'");
    };
    let root = find_sm83_tennant_dir().expect("SM83 corpus should exist");
    let (pass, fail, first_failures) = run_opcode_tests(&opcode_path(&root, opcode));

    println!("opcode {opcode:02X}: {pass} passed, {fail} failed");
    for failure in &first_failures {
        println!("{failure}");
    }
    assert_eq!(fail, 0, "opcode {opcode:02X} reported {fail} failures");
}

/// Runs the entire 256-opcode top-level corpus (skipping the illegal
/// opcodes the corpus doesn't ship).
#[test]
#[ignore = "FIXTURE: requires local Adam Tennant SM83 corpus and runs for ~minutes"]
fn run_all() {
    let root = find_sm83_tennant_dir().expect("SM83 corpus should exist");

    let mut pass = 0usize;
    let mut fail = 0usize;
    let mut missing = 0usize;
    let mut first_failures = Vec::new();

    println!("=== Adam Tennant SM83 single-step tests ===");
    println!("Corpus root: {}", root.display());

    for opcode in 0u16..=0xFF {
        let opcode = opcode as u8;
        let path = opcode_path(&root, opcode);
        if !path.exists() {
            missing += 1;
            continue;
        }
        let (opcode_pass, opcode_fail, failures) = run_opcode_tests(&path);
        pass += opcode_pass;
        fail += opcode_fail;
        if opcode_fail != 0 && first_failures.len() < 12 {
            first_failures.push(format!(
                "FAIL {opcode:02X}: {opcode_fail}/{} failed, first:\n  {}",
                opcode_pass + opcode_fail,
                failures
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "no failure details".to_owned())
            ));
        }
    }

    println!(
        "Adam Tennant SM83 compatibility: {pass} passed, {fail} failed \
         ({missing} opcodes have no corpus file — illegal/HALT/STOP/EI/DI)"
    );
    for failure in &first_failures {
        println!("{failure}");
    }
    assert_eq!(fail, 0, "SM83 corpus reported {fail} failures");
}

/// Runs the 25,600 entries in `cb.json` covering every `CB xx`
/// permutation.
#[test]
#[ignore = "FIXTURE: requires local Adam Tennant SM83 corpus and runs for ~minutes"]
fn run_cb() {
    let root = find_sm83_tennant_dir().expect("SM83 corpus should exist");
    let (pass, fail, first_failures) = run_opcode_tests(&cb_path(&root));

    println!("CB sub-table: {pass} passed, {fail} failed");
    for failure in &first_failures {
        println!("{failure}");
    }
    assert_eq!(fail, 0, "CB sub-table reported {fail} failures");
}
