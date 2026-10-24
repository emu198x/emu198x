//! The SingleStepTests 65816 corpus (MIT), cycle by cycle.
//!
//! Each case gives the registers and memory before and after one
//! instruction, and every cycle's address, data and bus status in between.
//! This harness checks all of them: address, read or write, the data driven
//! on a write, VDA, VPA, VPB, E, M, X and MLB, then the final registers and
//! memory.
//!
//! The corpus is 2.8 GB, so it is not in the repository. Point
//! `EMU198X_65816_SST_DIR` at a directory of its `v1/*.json` files, all of
//! them or any subset; the test runs whatever it finds.
//! `EMU198X_65816_SST_FILES` narrows a run to named stems, such as
//! `a9.n,a9.e`. Fetch the files from
//! <https://github.com/SingleStepTests/65816>. Run in release mode; it is
//! about ten million cases.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use emu198x_wdc_65c816::Wdc65c816;
use serde::Deserialize;

const DIR_ENV: &str = "EMU198X_65816_SST_DIR";
const FILES_ENV: &str = "EMU198X_65816_SST_FILES";

#[derive(Deserialize)]
struct Case {
    name: String,
    initial: State,
    #[serde(rename = "final")]
    final_state: State,
    cycles: Vec<(Option<u32>, Option<u8>, String)>,
}

#[derive(Deserialize)]
struct State {
    pc: u16,
    s: u16,
    p: u8,
    a: u16,
    x: u16,
    y: u16,
    dbr: u8,
    d: u16,
    pbr: u8,
    e: u8,
    ram: Vec<(u32, u8)>,
}

fn corpus_dir() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(dir) = std::env::var_os(DIR_ENV) {
        candidates.push(PathBuf::from(dir));
    }
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(
            PathBuf::from(home).join("Projects/198x/assets/test-suites/processor-tests/65816/v1"),
        );
    }
    candidates.into_iter().find(|dir| dir.is_dir())
}

fn run_case(case: &Case) -> Vec<String> {
    let mut mem: HashMap<u32, u8> = case.initial.ram.iter().copied().collect();
    let mut cpu = Wdc65c816::new();
    let i = &case.initial;
    cpu.regs.a = i.a;
    cpu.regs.x = i.x;
    cpu.regs.y = i.y;
    cpu.regs.s = i.s;
    cpu.regs.d = i.d;
    cpu.regs.dbr = i.dbr;
    cpu.regs.p = i.p;
    cpu.regs.e = i.e != 0;
    cpu.begin_at(i.pbr, i.pc);

    let mut errors = Vec::new();
    for (n, (addr, value, flags)) in case.cycles.iter().enumerate() {
        let f: Vec<char> = flags.chars().collect();
        match addr {
            None => {
                // WAI and STP leave the bus with no valid address.
                if cpu.bus_active() {
                    errors.push(format!("cycle{n}: bus active, expected none"));
                }
            }
            Some(addr) => {
                let want_read = f[3] == 'r';
                let checks = [
                    ("addr", cpu.addr == *addr),
                    ("rw", cpu.rw == want_read),
                    ("vda", cpu.vda == (f[0] == 'd')),
                    ("vpa", cpu.vpa == (f[1] == 'p')),
                    ("vpb", cpu.vpb == (f[2] == 'v')),
                    ("e", cpu.e() == (f[4] == 'e')),
                    ("m", cpu.m() == (f[5] == 'm')),
                    ("x", cpu.x() == (f[6] == 'x')),
                    ("mlb", cpu.mlb == (f[7] == 'l')),
                ];
                for (what, ok) in checks {
                    if !ok {
                        errors.push(format!(
                            "cycle{n}.{what}: got addr {:06X} {} vda{} vpa{} vpb{} e{} m{} x{} ml{}, expected {addr:06X} {flags}",
                            cpu.addr,
                            if cpu.rw { "r" } else { "w" },
                            u8::from(cpu.vda),
                            u8::from(cpu.vpa),
                            u8::from(cpu.vpb),
                            u8::from(cpu.e()),
                            u8::from(cpu.m()),
                            u8::from(cpu.x()),
                            u8::from(cpu.mlb),
                        ));
                    }
                }
                if cpu.rw {
                    cpu.data_in = mem.get(&cpu.addr).copied().unwrap_or(0);
                } else {
                    if Some(cpu.data) != *value {
                        errors.push(format!(
                            "cycle{n}.data: wrote {:02X}, expected {value:02X?}",
                            cpu.data
                        ));
                    }
                    mem.insert(cpu.addr, cpu.data);
                }
            }
        }
        if !errors.is_empty() {
            return errors;
        }
        cpu.tick();
    }

    let f = &case.final_state;
    let r = &cpu.regs;
    let regs = [
        ("pc", u32::from(r.pc), u32::from(f.pc)),
        ("pbr", u32::from(r.pbr), u32::from(f.pbr)),
        ("s", u32::from(r.s), u32::from(f.s)),
        ("p", u32::from(r.p), u32::from(f.p)),
        ("a", u32::from(r.a), u32::from(f.a)),
        ("x", u32::from(r.x), u32::from(f.x)),
        ("y", u32::from(r.y), u32::from(f.y)),
        ("d", u32::from(r.d), u32::from(f.d)),
        ("dbr", u32::from(r.dbr), u32::from(f.dbr)),
        ("e", u32::from(r.e), u32::from(f.e)),
    ];
    for (what, got, want) in regs {
        if got != want {
            errors.push(format!("{what}: got {got:#X}, expected {want:#X}"));
        }
    }
    for &(addr, want) in &f.ram {
        let got = mem.get(&addr).copied().unwrap_or(0);
        if got != want {
            errors.push(format!(
                "ram[{addr:06X}]: got {got:02X}, expected {want:02X}"
            ));
        }
    }
    errors
}

/// Run one corpus file, returning the pass and fail counts and the first
/// few failures.
fn run_file(path: &Path) -> (usize, usize, Vec<String>) {
    let text = std::fs::read_to_string(path).expect("read corpus file");
    let cases: Vec<Case> = serde_json::from_str(&text).expect("parse corpus file");
    let mut pass = 0;
    let mut fail = 0;
    let mut first = Vec::new();
    for case in &cases {
        let errors = run_case(case);
        if errors.is_empty() {
            pass += 1;
        } else {
            fail += 1;
            if first.len() < 3 {
                first.push(format!("{}: {}", case.name, errors.join("; ")));
            }
        }
    }
    (pass, fail, first)
}

#[test]
fn single_step_tests() {
    let Some(dir) = corpus_dir() else {
        emu198x_test_skip::skip!(
            "SingleStepTests 65816 corpus not present (set EMU198X_65816_SST_DIR)"
        );
    };
    let wanted: Option<Vec<String>> = std::env::var(FILES_ENV)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(|v| v.split(',').map(|s| s.trim().to_owned()).collect());
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("read corpus directory")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .filter(|path| {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            wanted.as_ref().is_none_or(|w| w.iter().any(|x| x == stem))
        })
        .collect();
    files.sort();
    if files.is_empty() {
        emu198x_test_skip::skip!("no corpus files matched in {}", dir.display());
    }

    let mut total_fail = 0;
    let mut total_pass = 0;
    for path in &files {
        let (pass, fail, first) = run_file(path);
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
        println!("{stem}: {pass} passed, {fail} failed");
        for failure in first {
            println!("    {failure}");
        }
        total_pass += pass;
        total_fail += fail;
    }
    println!("total: {total_pass} passed, {total_fail} failed");
    assert_eq!(total_fail, 0, "{total_fail} SingleStepTests cases failed");
}
