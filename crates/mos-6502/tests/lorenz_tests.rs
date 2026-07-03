/// Wolfgang Lorenz 6502 test harness.
///
/// Runs the CPU-focused subset of Wolfgang Lorenz's C64-based 6502 suite
/// against the fresh-workspace `mos-6502` core.
///
/// Run one smoke case:
/// `cargo test -p mos-6502 --test lorenz_tests run_lorenz_6502_smoke_ldab -- --ignored --nocapture`
///
/// Run one named case:
/// `EMU198X_6502_LORENZ_CASE=arrb cargo test -p mos-6502 --test lorenz_tests run_lorenz_6502_case -- --ignored --nocapture`
///
/// Run the full CPU subset:
/// `cargo test -p mos-6502 --test lorenz_tests run_lorenz_6502_cpu_suite -- --ignored --nocapture`
mod support;

use std::fmt::Write as _;
use std::fs;

use mos_6502::M6502;
use mos_6502::registers::FLAG_I;
use support::{find_c64_kernal_rom, find_lorenz_6502_dir};

const KERNAL_BASE: usize = 0xE000;
const TRAP_PRINT_CHAR: u16 = 0xFFD2;
const TRAP_SCAN_KEYBOARD: u16 = 0xFFE4;
const TRAP_FAIL_1: u16 = 0x8000;
const TRAP_FAIL_2: u16 = 0xA474;
const TRAP_SUCCESS: u16 = 0xE16F;
const DEFAULT_SAFETY_CYCLE_BUDGET: u64 = 20_000_000;
const FLOW_BRANCH_SAFETY_CYCLE_BUDGET: u64 = 50_000_000;
const ADC_SBC_SAFETY_CYCLE_BUDGET: u64 = 1_500_000_000;
const CASE_ENV: &str = "EMU198X_6502_LORENZ_CASE";
const LIMIT_ENV: &str = "EMU198X_6502_LORENZ_LIMIT";
const CYCLE_BUDGET_ENV: &str = "EMU198X_6502_LORENZ_CYCLE_BUDGET";

struct LorenzHarness {
    cpu: M6502,
    mem: [u8; 0x10000],
    output: Vec<u8>,
    last_opcode_addr: u16,
    /// External pin state for the 6510 zero-page port at `$0001`.
    /// Updated only on writes to `$01` for bits where the DDR
    /// (`$00`) is 1 — i.e. the CPU is actually driving the pin.
    /// Reads compose this with pull-up / no-pin masks; see
    /// [`Self::read_with_6510_port`].
    pin_state_01: u8,
}

enum StepOutcome {
    Continue,
    Success,
    Failure(String),
}

impl LorenzHarness {
    fn new(program_bytes: &[u8], kernal_rom: &[u8]) -> Result<Self, String> {
        if program_bytes.len() < 2 {
            return Err("Lorenz test file is too short to contain a load address".to_owned());
        }
        if kernal_rom.len() < 0x2000 {
            return Err(format!(
                "expected at least 8192 bytes of C64 KERNAL ROM, got {}",
                kernal_rom.len()
            ));
        }

        let mut mem = [0u8; 0x10000];
        let load_addr = u16::from_le_bytes([program_bytes[0], program_bytes[1]]) as usize;
        let contents = &program_bytes[2..];
        let end = load_addr
            .checked_add(contents.len())
            .ok_or_else(|| "Lorenz test file load address overflowed memory".to_owned())?;
        if end > mem.len() {
            return Err(format!(
                "Lorenz test file would overrun memory: ${load_addr:04X}..${:04X}",
                end - 1
            ));
        }

        mem[load_addr..end].copy_from_slice(contents);
        mem[KERNAL_BASE..KERNAL_BASE + 0x2000].copy_from_slice(&kernal_rom[..0x2000]);

        // Match the setup from the long-standing Lorenz harness used in other
        // projects: default vectors, IRQ stub, and self-loop at the KERNAL
        // load routine to signal success.
        mem[0xD011] = 0xFF;
        mem[0x0316] = 0x66;
        mem[0x0317] = 0xFE;
        mem[0x0314] = 0x31;
        mem[0x0315] = 0xEA;
        mem[0x0002] = 0x00;
        mem[0xA002] = 0x00;
        mem[0xA003] = 0x80;
        mem[0x01FE] = 0xFF;
        mem[0x01FF] = 0x7F;
        mem[0xFFFE] = 0x48;
        mem[0xFFFF] = 0xFF;

        let irq_handler: [u8; 19] = [
            0x48, 0x8A, 0x48, 0x98, 0x48, 0xBA, 0xBD, 0x04, 0x01, 0x29, 0x10, 0xF0, 0x03, 0x6C,
            0x16, 0x03, 0x6C, 0x14, 0x03,
        ];
        mem[0xFF48..0xFF48 + irq_handler.len()].copy_from_slice(&irq_handler);

        mem[TRAP_PRINT_CHAR as usize] = 0x60;
        mem[TRAP_SCAN_KEYBOARD as usize] = 0x60;
        mem[TRAP_FAIL_1 as usize] = 0x60;
        mem[TRAP_FAIL_2 as usize] = 0x60;
        mem[TRAP_SUCCESS as usize] = 0x4C;
        mem[TRAP_SUCCESS as usize + 1] = 0x6F;
        mem[TRAP_SUCCESS as usize + 2] = 0xE1;

        mem[0xFFFC] = 0x01;
        mem[0xFFFD] = 0x08;

        let mut cpu = M6502::new();
        cpu.reset();

        let mut harness = Self {
            cpu,
            mem,
            output: Vec::new(),
            last_opcode_addr: 0x0000,
            pin_state_01: 0x00,
        };
        harness.complete_reset();
        harness.cpu.regs.a = 0x00;
        harness.cpu.regs.x = 0x00;
        harness.cpu.regs.y = 0x00;
        harness.cpu.regs.sp = 0xFD;
        harness.cpu.regs.p = FLAG_I;
        harness.cpu.total_cycles = 0;

        Ok(harness)
    }

    fn complete_reset(&mut self) {
        while !(self.cpu.instruction_complete() && self.cpu.sync && self.cpu.addr == 0x0801) {
            self.step_bus_only();
        }
    }

    fn run_until_terminal(&mut self, safety_cycle_budget: u64) -> Result<u64, String> {
        while self.cpu.total_cycles < safety_cycle_budget {
            match self.step() {
                StepOutcome::Continue => {}
                StepOutcome::Success => return Ok(self.cpu.total_cycles),
                StepOutcome::Failure(message) => return Err(message),
            }
        }

        Err(format!(
            "Lorenz test exceeded safety budget of {safety_cycle_budget} cycles; last opcode at ${:04X}; output: {}",
            self.last_opcode_addr,
            self.petscii_output()
        ))
    }

    fn step(&mut self) -> StepOutcome {
        if self.cpu.sync && self.cpu.rw {
            self.last_opcode_addr = self.cpu.addr;
            match self.cpu.addr {
                TRAP_PRINT_CHAR => {
                    self.mem[0x030C] = 0x00;
                    self.output.push(self.cpu.regs.a);
                }
                TRAP_SCAN_KEYBOARD => {
                    self.cpu.regs.a = 0x03;
                }
                TRAP_FAIL_1 | TRAP_FAIL_2 => {
                    return StepOutcome::Failure(format!(
                        "Lorenz test reported failure at ${:04X}: {}",
                        self.cpu.addr,
                        self.petscii_output()
                    ));
                }
                0x0000 => {
                    return StepOutcome::Failure("execution hit $0000".to_owned());
                }
                TRAP_SUCCESS => return StepOutcome::Success,
                _ => {}
            }
        }

        self.step_bus_only();
        if self.cpu.halted {
            return StepOutcome::Failure(format!(
                "processor jammed unexpectedly at ${:04X}; output: {}",
                self.last_opcode_addr,
                self.petscii_output()
            ));
        }

        StepOutcome::Continue
    }

    fn step_bus_only(&mut self) {
        if self.cpu.rw {
            self.cpu.data_in = self.read_with_6510_port(self.cpu.addr);
        } else {
            self.mem[self.cpu.addr as usize] = self.cpu.data;
            if self.cpu.addr == 0x0000 || self.cpu.addr == 0x0001 {
                self.update_pin_state_01();
            }
        }
        self.cpu.tick();
    }

    /// Recompute the external pin-state snapshot for `$0001`.
    ///
    /// For each bit where the DDR (`$0000`) is 1 the pin is being
    /// driven by `mem[$0001]`, so we overlay that value onto
    /// `pin_state_01`. For each bit where the DDR is 0 the pin is
    /// floating; we leave `pin_state_01` untouched so it preserves
    /// the last driven value (capacitor memory). Called after any
    /// write to `$00` or `$01`.
    fn update_pin_state_01(&mut self) {
        let ddr = self.mem[0x0000];
        let mem01 = self.mem[0x0001];
        self.pin_state_01 = (self.pin_state_01 & !ddr) | (mem01 & ddr);
    }

    /// Read with 6510 zero-page-port semantics at `$0001`.
    ///
    /// Output bits (DDR=1) return the value the CPU last wrote
    /// (which is what sits in memory). Input bits (DDR=0) compose:
    /// pull-ups on bits 0-2 + 4 (LORAM, HIRAM, CHAREN, CASS_SENSE)
    /// always read high once their capacitors decay (modelled as
    /// instantaneous); bit 5 (CASS_MOTOR) has an external load that
    /// drags it back to ground when no longer driven, so it reads
    /// as a pull-down; bits 3, 6, 7 float and retain the last
    /// driven value (capacitor memory tracked in `pin_state_01`,
    /// kept in sync by [`Self::update_pin_state_01`]).
    ///
    /// That gives `$17` when the pins were last driven low and
    /// `$DF` when they were last driven high — both patterns
    /// Lorenz's `cpuport` test compares against.
    fn read_with_6510_port(&self, addr: u16) -> u8 {
        if addr == 0x0001 {
            const PULL_UPS: u8 = 0x17;
            const PULL_DOWNS: u8 = 0x20;
            let ddr = self.mem[0x0000];
            let written = self.mem[0x0001];
            let pins = PULL_UPS | (self.pin_state_01 & !PULL_DOWNS);
            (written & ddr) | (pins & !ddr)
        } else {
            self.mem[addr as usize]
        }
    }

    fn petscii_output(&self) -> String {
        let mut output = String::new();
        for &byte in &self.output {
            match byte {
                0x0D => output.push('\n'),
                0x20..=0x5A => output.push(byte as char),
                0x5B => output.push('['),
                0x5C => output.push('\\'),
                0x5D => output.push(']'),
                0x5E => output.push('^'),
                0x5F => output.push('_'),
                0x61..=0x7A => output.push((byte - 0x20) as char),
                _ => {
                    let _ = write!(output, "<{byte:02X}>");
                }
            }
        }
        output
    }
}

fn load_lorenz_program(name: &str) -> Result<Vec<u8>, String> {
    let path = find_lorenz_6502_dir()?.join(name);
    fs::read(&path).map_err(|error| format!("failed to read {}: {error}", path.display()))
}

fn load_kernal_rom() -> Result<Vec<u8>, String> {
    let path = find_c64_kernal_rom()?;
    fs::read(&path).map_err(|error| format!("failed to read {}: {error}", path.display()))
}

fn cycle_budget_from_env() -> Option<u64> {
    match std::env::var(CYCLE_BUDGET_ENV) {
        Ok(value) if !value.trim().is_empty() => {
            Some(value.trim().parse().unwrap_or_else(|error| {
                panic!("failed to parse {CYCLE_BUDGET_ENV}='{value}': {error}")
            }))
        }
        _ => None,
    }
}

fn safety_cycle_budget(name: &str) -> u64 {
    match cycle_budget_from_env() {
        Some(budget) => budget,
        None if name.starts_with("adc") || name.starts_with("sbc") || name.starts_with("anc") => {
            ADC_SBC_SAFETY_CYCLE_BUDGET
        }
        None if matches!(
            name,
            "brkn"
                | "rtin"
                | "jsrw"
                | "rtsn"
                | "jmpw"
                | "jmpi"
                | "beqr"
                | "bner"
                | "bmir"
                | "bplr"
                | "bcsr"
                | "bccr"
                | "bvsr"
                | "bvcr"
        ) =>
        {
            FLOW_BRANCH_SAFETY_CYCLE_BUDGET
        }
        None => DEFAULT_SAFETY_CYCLE_BUDGET,
    }
}

fn run_case(name: &str, kernal_rom: &[u8]) -> Result<u64, String> {
    let program = load_lorenz_program(name)?;
    let mut harness = LorenzHarness::new(&program, kernal_rom)?;
    harness.run_until_terminal(safety_cycle_budget(name))
}

fn cpu_test_names() -> Vec<String> {
    let mut tests = Vec::new();

    tests.push(" start".to_owned());
    extend(
        &mut tests,
        "lda",
        &["b", "z", "zx", "a", "ax", "ay", "ix", "iy"],
    );
    extend(&mut tests, "sta", &["z", "zx", "a", "ax", "ay", "ix", "iy"]);
    extend(&mut tests, "ldx", &["b", "z", "zy", "a", "ay"]);
    extend(&mut tests, "stx", &["z", "zy", "a"]);
    extend(&mut tests, "ldy", &["b", "z", "zx", "a", "ax"]);
    extend(&mut tests, "sty", &["z", "zx", "a"]);
    extend(
        &mut tests,
        "",
        &["taxn", "tayn", "txan", "tyan", "tsxn", "txsn"],
    );
    extend(&mut tests, "", &["phan", "plan", "phpn", "plpn"]);
    extend(
        &mut tests,
        "",
        &[
            "inxn", "inyn", "dexn", "deyn", "incz", "inczx", "inca", "incax", "decz", "deczx",
            "deca", "decax",
        ],
    );
    extend(&mut tests, "asl", &["n", "z", "zx", "a", "ax"]);
    extend(&mut tests, "lsr", &["n", "z", "zx", "a", "ax"]);
    extend(&mut tests, "rol", &["n", "z", "zx", "a", "ax"]);
    extend(&mut tests, "ror", &["n", "z", "zx", "a", "ax"]);
    extend(
        &mut tests,
        "and",
        &["b", "z", "zx", "a", "ax", "ay", "ix", "iy"],
    );
    extend(
        &mut tests,
        "ora",
        &["b", "z", "zx", "a", "ax", "ay", "ix", "iy"],
    );
    extend(
        &mut tests,
        "eor",
        &["b", "z", "zx", "a", "ax", "ay", "ix", "iy"],
    );
    extend(
        &mut tests,
        "",
        &["clcn", "secn", "cldn", "sedn", "clin", "sein", "clvn"],
    );
    extend(
        &mut tests,
        "adc",
        &["b", "z", "zx", "a", "ax", "ay", "ix", "iy"],
    );
    extend(
        &mut tests,
        "sbc",
        &["b", "z", "zx", "a", "ax", "ay", "ix", "iy"],
    );
    extend(
        &mut tests,
        "cmp",
        &["b", "z", "zx", "a", "ax", "ay", "ix", "iy"],
    );
    extend(&mut tests, "cpx", &["b", "z", "a"]);
    extend(&mut tests, "cpy", &["b", "z", "a"]);
    extend(&mut tests, "bit", &["z", "a"]);
    extend(
        &mut tests,
        "",
        &["brkn", "rtin", "jsrw", "rtsn", "jmpw", "jmpi"],
    );
    extend(
        &mut tests,
        "",
        &[
            "beqr", "bner", "bmir", "bplr", "bcsr", "bccr", "bvsr", "bvcr",
        ],
    );
    extend(&mut tests, "nop", &["n", "b", "z", "zx", "a", "ax"]);
    extend(&mut tests, "aso", &["z", "zx", "a", "ax", "ay", "ix", "iy"]);
    extend(&mut tests, "rla", &["z", "zx", "a", "ax", "ay", "ix", "iy"]);
    extend(&mut tests, "lse", &["z", "zx", "a", "ax", "ay", "ix", "iy"]);
    extend(&mut tests, "rra", &["z", "zx", "a", "ax", "ay", "ix", "iy"]);
    extend(&mut tests, "dcm", &["z", "zx", "a", "ax", "ay", "ix", "iy"]);
    extend(&mut tests, "ins", &["z", "zx", "a", "ax", "ay", "ix", "iy"]);
    extend(&mut tests, "lax", &["z", "zy", "a", "ay", "ix", "iy"]);
    extend(&mut tests, "axs", &["z", "zy", "a", "ix"]);
    extend(
        &mut tests,
        "",
        &[
            "alrb", "arrb", "sbxb", "shxay", "shyax", "shsay", "lxab", "aneb", "ancb", "lasay",
            "sbcb(eb)",
        ],
    );
    extend(&mut tests, "sha", &["ay", "iy"]);

    tests
}

fn extend(target: &mut Vec<String>, stem: &str, suffixes: &[&str]) {
    target.extend(suffixes.iter().map(|suffix| format!("{stem}{suffix}")));
}

fn selected_case_from_env() -> Option<String> {
    match std::env::var(CASE_ENV) {
        Ok(value) if !value.trim().is_empty() => Some(value),
        _ => None,
    }
}

fn selected_limit_from_env() -> Option<usize> {
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
fn lorenz_cpu_test_inventory_contains_key_cases() {
    let names = cpu_test_names();
    assert!(names.iter().any(|name| name == "ldab"));
    assert!(names.iter().any(|name| name == "arrb"));
    assert!(names.iter().any(|name| name == "jmpi"));
    assert!(names.iter().any(|name| name == "sbcb(eb)"));
    assert!(!names.iter().any(|name| name == "cpuport"));
    assert!(!names.iter().any(|name| name == "cputiming"));
}

#[test]
#[ignore = "requires local Wolfgang Lorenz 6502 suite and C64 KERNAL ROM"]
fn run_lorenz_6502_smoke_ldab() {
    let kernal = load_kernal_rom().expect("local C64 KERNAL ROM should be available");
    let cycles = run_case("ldab", &kernal).expect("ldab should complete successfully");
    println!("Lorenz smoke ldab passed in {cycles} cycles");
}

#[test]
#[ignore = "requires local Wolfgang Lorenz 6502 suite and C64 KERNAL ROM"]
fn run_lorenz_6502_case() {
    let Some(case_name) = selected_case_from_env() else {
        panic!("set {CASE_ENV} to one Lorenz case name, for example 'arrb' or 'jmpi'");
    };
    let kernal = load_kernal_rom().expect("local C64 KERNAL ROM should be available");
    let cycles = run_case(&case_name, &kernal)
        .unwrap_or_else(|message| panic!("Lorenz case '{case_name}' failed: {message}"));
    println!("Lorenz case {case_name} passed in {cycles} cycles");
}

#[test]
#[ignore = "requires local Wolfgang Lorenz 6502 suite and runs for minutes"]
fn run_lorenz_6502_cpu_suite() {
    let kernal = load_kernal_rom().expect("local C64 KERNAL ROM should be available");
    let mut tests = cpu_test_names();
    if let Some(limit) = selected_limit_from_env() {
        tests.truncate(limit);
    }

    let total = tests.len();
    let mut pass = 0usize;
    let mut fail = 0usize;
    let mut first_failures = Vec::new();

    println!("=== Wolfgang Lorenz 6502 CPU Tests ===");
    println!(
        "Suite root: {}",
        find_lorenz_6502_dir()
            .expect("suite should exist")
            .display()
    );
    println!(
        "KERNAL ROM: {}",
        find_c64_kernal_rom_path_for_report().display()
    );
    println!("Running {total} cases");

    for name in tests {
        match run_case(&name, &kernal) {
            Ok(cycles) => {
                pass += 1;
                if pass <= 5 {
                    println!("PASS {name} ({cycles} cycles)");
                }
            }
            Err(message) => {
                fail += 1;
                if first_failures.len() < 8 {
                    first_failures.push(format!("FAIL {name}: {message}"));
                }
            }
        }
    }

    println!("Lorenz 6502 CPU subset: {pass}/{total} passed, {fail} failed");
    for failure in &first_failures {
        println!("{failure}");
    }

    assert_eq!(fail, 0, "Lorenz 6502 CPU subset reported {fail} failures");
}

fn find_c64_kernal_rom_path_for_report() -> std::path::PathBuf {
    find_c64_kernal_rom().expect("local C64 KERNAL ROM should be available")
}

// ════════════════════════════════════════════════════════════════
//  Lorenz sweep — `nes_sweep`-style coverage over the ENTIRE Lorenz
//  suite, not just the curated CPU subset.
//
//  Each test in the suite directory is run through the same
//  KERNAL-trap harness and categorised:
//
//    PASS    — Reached `TRAP_SUCCESS`.
//    FAIL    — Reached a fail trap, hit a JAM, or printed an
//              explicit `*** FAIL` body.
//    SKIP    — Requires a feature the CPU-only harness doesn't
//              model (CIA timer IRQs, raster interrupts, NMI/IRQ
//              gating, MMU/zero-page port quirks). Listed
//              explicitly in `KNOWN_HARDWARE_DEPENDENT` so the
//              sweep number reflects "what the CPU passes" rather
//              than "what blargg-style ROMs we can hand-grade."
//
//  Run with:
//    cargo test --release -p mos-6502 --test lorenz_tests \
//        lorenz_sweep -- --ignored --nocapture
// ════════════════════════════════════════════════════════════════

/// Lorenz cases that fundamentally need a real C64 machine. Each
/// entry was empirically verified to FAIL in the CPU-only +
/// KERNAL-trap harness and PASS in a real C64 (per Lorenz's
/// published expected results).
///
/// These are no longer just "skips": the full-machine harness at
/// `crates/runtime-commodore-c64/tests/lorenz_machine.rs` (issue #18)
/// runs this exact set against the real board and scores each one.
/// `cputiming` and `mmufetch` already pass there; the CIA-timer /
/// `irq` / `nmi` cases await the CIA cycle-delay pipeline (#17), and
/// `mmu` surfaced a distinct banking gap. Keep the two lists in sync.
///
/// The categories below are the natural sub-tasks for any future
/// "make the C64 machine cycle-accurate" session: each name calls
/// out exactly which feature it probes.
///
/// Tests previously assumed-hardware that turned out to run fine
/// in the CPU harness — `branchwrap`, `cntdef`, `cnto2`, `flipos`,
/// `icr01`, `imr`, `oneshot`, `loadth`, `cia1pb6` / `cia1pb7` /
/// `cia2pb6` / `cia2pb7` (CIA port-bit static reads), `trap1`
/// through `trap15` — were moved out of this list to reflect
/// actual behaviour.
const KNOWN_HARDWARE_DEPENDENT: &[&str] = &[
    // CIA timer A / B internals + interaction.
    "cia1ta",
    "cia1tab",
    "cia1tb",
    "cia1tb123",
    "cia2ta",
    "cia2tb",
    "cia2tb123",
    // CPU-side IRQ / NMI gating, NMI taken-priority-over-IRQ.
    "irq",
    "nmi",
    // CPU bus timing variants the CPU-only harness can't measure.
    "cputiming",
    // 6510 MMU / banking — the harness has a flat memory map.
    "mmu",
    "mmufetch",
    // Last two of the 17 per-opcode sweeps place the opcode at
    // $FFFE/$FFFF, wrapping the PC through the 6510 port at
    // $0000/$0001 — needs the real machine, not a flat map.
    "trap16",
    "trap17",
    // Lorenz's `finish` is a final synthesizer that drives the
    // KERNAL screen-clear routine; the CPU harness can't carry it
    // through.
    "finish",
];

#[test]
#[ignore = "long survey; run with --release --ignored --nocapture"]
fn lorenz_sweep() {
    let suite_dir = match find_lorenz_6502_dir() {
        Ok(d) => d,
        Err(message) => {
            eprintln!("Wolfgang Lorenz suite not found; skipping sweep: {message}");
            return;
        }
    };
    let kernal = match load_kernal_rom() {
        Ok(k) => k,
        Err(message) => {
            eprintln!("C64 KERNAL ROM not found; skipping sweep: {message}");
            return;
        }
    };

    eprintln!("=== Wolfgang Lorenz 6502 Sweep ===");
    eprintln!("Suite root: {}", suite_dir.display());

    let mut entries: Vec<String> = match fs::read_dir(&suite_dir) {
        Ok(rd) => rd
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                if p.is_file() {
                    p.file_name().and_then(|n| n.to_str()).map(String::from)
                } else {
                    None
                }
            })
            .filter(|name| !name.starts_with('.'))
            .filter(|name| !name.ends_with(".md"))
            .filter(|name| !name.ends_with(".txt"))
            .filter(|name| !name.ends_with(".swift"))
            .collect(),
        Err(err) => {
            eprintln!("read_dir failed: {err}");
            return;
        }
    };
    entries.sort();

    let total = entries.len();
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut first_failures: Vec<String> = Vec::new();

    for name in &entries {
        let label = name.as_str();
        // Lorenz includes one file literally called " start"
        // (leading space) as the suite preamble. Preserve the
        // raw filename for `run_case`; only trim for the
        // hardware-dependent skip-list lookup.
        if KNOWN_HARDWARE_DEPENDENT.contains(&label.trim()) {
            skipped += 1;
            eprintln!("  SKIP    {label:<24} (hardware-dependent — needs full C64 machine)");
            continue;
        }
        match run_case(label, &kernal) {
            Ok(cycles) => {
                passed += 1;
                eprintln!("  PASS    {label:<24} ({cycles} cycles)");
            }
            Err(message) => {
                failed += 1;
                let trimmed: String = message
                    .lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(80)
                    .collect();
                eprintln!("  FAIL    {label:<24} — {trimmed}");
                if first_failures.len() < 16 {
                    first_failures.push(format!("FAIL {label}: {message}"));
                }
            }
        }
    }

    eprintln!("\n=== LORENZ SWEEP SUMMARY ===");
    eprintln!(
        "Total: {total}  Pass: {passed}  Fail: {failed}  Skip (hardware-dependent): {skipped}"
    );
    if !first_failures.is_empty() {
        eprintln!("\nFirst failures:");
        for failure in &first_failures {
            eprintln!("  {failure}");
        }
    }
}
