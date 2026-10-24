//! Every instruction against SingleStepTests/spc700, cycle by cycle.
//!
//! The corpus (MIT licence, <https://github.com/SingleStepTests/spc700>) is
//! 97 MB of JSON, so it is fetched rather than committed:
//!
//! ```text
//! git clone --depth 1 https://github.com/SingleStepTests/spc700 \
//!     ~/.emu198x/test-data/SingleStepTests/spc700
//! ```
//!
//! or point `SPC700_TEST_DATA` at its `v1` folder. Each test sets up
//! registers and RAM, runs one instruction, and lists every bus cycle:
//! reads and writes with their address and data, and idle cycles. The CPU
//! must match all of them, and the registers and RAM after.

use std::collections::HashMap;
use std::path::PathBuf;

use emu198x_sony_spc700::{BusCycle, Flags, Halt, Registers, Spc700};
use serde_json::Value;

fn corpus_root() -> PathBuf {
    if let Ok(path) = std::env::var("SPC700_TEST_DATA") {
        return PathBuf::from(path);
    }
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".emu198x/test-data/SingleStepTests/spc700/v1")
}

fn number(v: &Value) -> u64 {
    v.as_u64().expect("a number")
}

fn state(v: &Value) -> (Registers, Vec<(u16, u8)>) {
    let regs = Registers {
        a: number(&v["a"]) as u8,
        x: number(&v["x"]) as u8,
        y: number(&v["y"]) as u8,
        sp: number(&v["sp"]) as u8,
        pc: number(&v["pc"]) as u16,
        p: Flags::from_bits(number(&v["psw"]) as u8),
        halt: Halt::None,
    };
    let ram = v["ram"]
        .as_array()
        .expect("ram")
        .iter()
        .map(|pair| (number(&pair[0]) as u16, number(&pair[1]) as u8))
        .collect();
    (regs, ram)
}

/// Run one test; `None` when it passes, else what differed first.
fn run(test: &Value) -> Option<String> {
    let (initial, ram) = state(&test["initial"]);
    let mut memory: HashMap<u16, u8> = ram.into_iter().collect();
    let mut cpu = Spc700::with_registers(initial);

    let cycles = test["cycles"].as_array().expect("cycles");
    for (i, expected) in cycles.iter().enumerate() {
        cpu.tick();
        let kind = expected[2].as_str().expect("kind");
        let got = cpu.bus_cycle();
        let ok = match (kind, got) {
            ("read", BusCycle::Read(addr)) => {
                u64::from(addr) == number(&expected[0])
                    && (expected[1].is_null()
                        || u64::from(*memory.get(&addr).unwrap_or(&0)) == number(&expected[1]))
            }
            ("write", BusCycle::Write(addr, data)) => {
                u64::from(addr) == number(&expected[0]) && u64::from(data) == number(&expected[1])
            }
            ("wait", BusCycle::Idle) => true,
            _ => false,
        };
        if !ok {
            return Some(format!("cycle {i}: expected {expected}, got {got:?}"));
        }
        match got {
            BusCycle::Read(addr) => cpu.data_in = *memory.get(&addr).unwrap_or(&0),
            BusCycle::Write(addr, data) => {
                memory.insert(addr, data);
            }
            BusCycle::Idle => {}
        }
    }
    // One more tick commits the instruction and starts the next.
    cpu.tick();

    let (fin, ram) = state(&test["final"]);
    let r = cpu.regs;
    let got = (r.a, r.x, r.y, r.sp, r.pc, r.p.bits());
    let want = (fin.a, fin.x, fin.y, fin.sp, fin.pc, fin.p.bits());
    if got != want {
        return Some(format!(
            "registers (a,x,y,sp,pc,psw): want {want:02X?}, got {got:02X?}"
        ));
    }
    for (addr, value) in ram {
        let have = *memory.get(&addr).unwrap_or(&0);
        if have != value {
            return Some(format!("ram ${addr:04X}: want {value:02X}, got {have:02X}"));
        }
    }
    None
}

#[test]
fn every_opcode_matches_single_step_tests() {
    let root = corpus_root();
    if !root.join("00.json").exists() {
        emu198x_test_skip::skip!(
            "SingleStepTests/spc700 corpus not staged: {} (or set SPC700_TEST_DATA)",
            root.display()
        );
    }
    let mut total = 0usize;
    let mut failed_opcodes = Vec::new();
    for opcode in 0..=0xFFu8 {
        let path = root.join(format!("{opcode:02x}.json"));
        let text = std::fs::read_to_string(&path).expect("read test file");
        let tests: Vec<Value> = serde_json::from_str(&text).expect("parse test file");
        let mut fails = 0;
        let mut first = None;
        for test in &tests {
            total += 1;
            if let Some(why) = run(test) {
                fails += 1;
                first.get_or_insert_with(|| format!("{}: {why}", test["name"]));
            }
        }
        if fails > 0 {
            failed_opcodes.push(format!(
                "${opcode:02X}: {fails}/{} failed, first {}",
                tests.len(),
                first.unwrap_or_default()
            ));
        }
    }
    println!("SingleStepTests/spc700: {total} tests run");
    assert!(
        failed_opcodes.is_empty(),
        "{} opcodes failed:\n{}",
        failed_opcodes.len(),
        failed_opcodes.join("\n")
    );
}
