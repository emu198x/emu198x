/// Tom Harte 6502 single-step test harness.
///
/// Runs the per-instruction JSON corpus for the NMOS 6502 and compares the
/// resulting register and memory state after one instruction completes.
///
/// Run one smoke opcode:
/// `cargo test -p mos-6502 --test single_step_tests run_opcode_69 -- --ignored --nocapture`
///
/// Run one named opcode from the environment:
/// `EMU198X_6502_OPCODE=69 cargo test -p mos-6502 --test single_step_tests run_named_opcode -- --ignored --nocapture`
///
/// Run the full corpus:
/// `cargo test -p mos-6502 --test single_step_tests run_all -- --ignored --nocapture`
mod support;

use std::path::Path;

use mos_6502::M6502;
use serde::Deserialize;
use support::find_tom_harte_6502_dir;

const OPCODE_ENV: &str = "EMU198X_6502_OPCODE";
const LIMIT_ENV: &str = "EMU198X_6502_LIMIT";

#[derive(Deserialize)]
struct TestCase {
    name: String,
    initial: State,
    #[serde(rename = "final")]
    final_state: State,
    cycles: Vec<(u16, u8, String)>,
}

#[derive(Deserialize)]
struct State {
    pc: u16,
    s: u8,
    a: u8,
    x: u8,
    y: u8,
    p: u8,
    ram: Vec<(u16, u8)>,
}

fn setup_cpu(cpu: &mut M6502, state: &State, mem: &[u8; 65536]) {
    cpu.regs.pc = state.pc;
    cpu.regs.sp = state.s;
    cpu.regs.a = state.a;
    cpu.regs.x = state.x;
    cpu.regs.y = state.y;
    cpu.regs.p = state.p;
    cpu.total_cycles = 0;
    cpu.addr = state.pc;
    cpu.data = 0;
    cpu.rw = true;
    cpu.sync = true;
    cpu.data_in = mem[state.pc as usize];
    cpu.irq = false;
    cpu.nmi = false;
    cpu.rdy = true;
    cpu.halted = false;
}

fn check_cpu(cpu: &M6502, expected: &State, mem: &[u8; 65536]) -> Vec<String> {
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

    check!("PC", cpu.regs.pc, expected.pc);
    check!("S", cpu.regs.sp as u16, expected.s as u16);
    check!("A", cpu.regs.a as u16, expected.a as u16);
    check!("X", cpu.regs.x as u16, expected.x as u16);
    check!("Y", cpu.regs.y as u16, expected.y as u16);
    check!("P", cpu.regs.p as u16, expected.p as u16);

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

fn run_test(test: &TestCase) -> Vec<String> {
    let mut cpu = M6502::new();
    let mut mem = [0u8; 65536];

    for &(addr, val) in &test.initial.ram {
        mem[addr as usize] = val;
    }
    for &(addr, data, ref kind) in &test.cycles {
        if kind != "write" {
            mem[addr as usize] = data;
        }
    }
    for &(addr, val) in &test.initial.ram {
        mem[addr as usize] = val;
    }

    setup_cpu(&mut cpu, &test.initial, &mem);

    for _ in 0..test.cycles.len() {
        if cpu.rw {
            cpu.data_in = mem[cpu.addr as usize];
        } else {
            mem[cpu.addr as usize] = cpu.data;
        }
        cpu.tick();
    }

    check_cpu(&cpu, &test.final_state, &mem)
}

fn run_opcode_tests(path: &Path) -> (usize, usize, Vec<String>) {
    let data = std::fs::read_to_string(path).expect("failed to read JSON opcode file");
    let tests: Vec<TestCase> =
        serde_json::from_str(&data).expect("failed to parse JSON opcode file");

    let mut pass = 0usize;
    let mut fail = 0usize;
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

    (pass, fail, first_failures)
}

fn opcode_path(root: &Path, opcode: u8) -> std::path::PathBuf {
    root.join(format!("{opcode:02x}.json"))
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

fn opcode_limit_from_env() -> Option<usize> {
    match std::env::var(LIMIT_ENV) {
        Ok(value) if !value.trim().is_empty() => Some(
            value
                .parse()
                .unwrap_or_else(|error| panic!("failed to parse {LIMIT_ENV}='{value}': {error}")),
        ),
        _ => None,
    }
}

#[test]
fn opcode_path_uses_lowercase_hex_names() {
    let path = opcode_path(Path::new("/tmp/6502"), 0x69);
    assert!(path.ends_with("69.json"));
}

#[test]
#[ignore = "requires local Tom Harte 6502 corpus"]
fn run_opcode_69() {
    let root = find_tom_harte_6502_dir().expect("Tom Harte 6502 corpus should exist");
    let (pass, fail, first_failures) = run_opcode_tests(&opcode_path(&root, 0x69));

    println!("opcode 69: {pass} passed, {fail} failed");
    for failure in &first_failures {
        println!("{failure}");
    }

    assert_eq!(fail, 0, "opcode 69 reported {fail} failures");
}

#[test]
#[ignore = "requires local Tom Harte 6502 corpus"]
fn run_named_opcode() {
    let Some(opcode) = named_opcode_from_env() else {
        panic!("set {OPCODE_ENV} to one hex opcode, for example '69' or 'e9'");
    };
    let root = find_tom_harte_6502_dir().expect("Tom Harte 6502 corpus should exist");
    let (pass, fail, first_failures) = run_opcode_tests(&opcode_path(&root, opcode));

    println!("opcode {opcode:02X}: {pass} passed, {fail} failed");
    for failure in &first_failures {
        println!("{failure}");
    }

    assert_eq!(fail, 0, "opcode {opcode:02X} reported {fail} failures");
}

#[test]
#[ignore = "requires local Tom Harte 6502 corpus and runs for minutes"]
fn run_all() {
    let root = find_tom_harte_6502_dir().expect("Tom Harte 6502 corpus should exist");
    let limit = opcode_limit_from_env();
    let total_opcodes = limit.unwrap_or(256).min(256);

    let mut pass = 0usize;
    let mut fail = 0usize;
    let mut first_failures = Vec::new();

    println!("=== Tom Harte 6502 Tests ===");
    println!("Corpus root: {}", root.display());
    println!("Running {total_opcodes} opcode files");

    for opcode in 0..total_opcodes {
        let opcode = opcode as u8;
        let path = opcode_path(&root, opcode);
        let (opcode_pass, opcode_fail, failures) = run_opcode_tests(&path);
        pass += opcode_pass;
        fail += opcode_fail;

        if opcode_fail == 0 && opcode < 4 {
            println!("PASS {opcode:02X}: {opcode_pass} tests");
        } else if opcode_fail != 0 && first_failures.len() < 12 {
            first_failures.push(format!(
                "FAIL {opcode:02X}: {opcode_fail} failures, first: {}",
                failures
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_owned())
            ));
        }
    }

    println!(
        "Tom Harte 6502 compatibility: {pass} passed, {fail} failed across {total_opcodes} opcode files"
    );
    for failure in &first_failures {
        println!("{failure}");
    }

    assert_eq!(fail, 0, "Tom Harte 6502 corpus reported {fail} failures");
}
