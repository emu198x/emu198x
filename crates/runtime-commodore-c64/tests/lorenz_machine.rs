//! Full-machine Wolfgang Lorenz harness (issue #18).
//!
//! The CPU-only Lorenz harness in `crates/mos-6502/tests/lorenz_tests.rs`
//! runs the bare 6502 against a flat memory map and *skips* the cases that
//! need a real C64 — the CIA timers, IRQ/NMI delivery, `cputiming`, the 6510
//! banking, and the tape KERNAL traps. This harness runs those same cases
//! against the real `machine-commodore-c64` board (live CIA ×2, VIC-II, IRQ
//! wiring, banking) so their pass/fail is *scored*, not hidden behind a skip.
//!
//! It is a **tracked ledger**, not a green-all gate. Today `cputiming` and
//! `mmufetch` pass; the seven CIA-timer cases plus `irq` / `nmi` print a
//! one-cycle-off register mismatch that needs the CIA cycle-delay pipeline
//! (#17), `mmu` prints a distinct banking mismatch, and the tape traps /
//! `finish` don't run standalone. The ledger asserts the passing set does not
//! regress and records the rest, so landing #17 (etc.) is a visible, closeable
//! move of a case from the red list to `EXPECTED_PASS`.
//!
//! How each case runs:
//!   1. Boot a real C64 to `READY.` once, then clone it per case (cheap; the
//!      boot cost is paid a single time).
//!   2. Load the Lorenz test PRG into RAM and point the KERNAL's RAM I/O
//!      vectors (`$0326` CHROUT, `$032A` GETIN, `$0328` STOP) at tiny RAM
//!      stubs, so the harness captures the printed output and feeds the
//!      keyboard/STOP lines without a live screen editor.
//!   3. Seed the CPU straight to the test entry (`$0801`, whose first bytes
//!      are `JMP` into the setup code) and step the whole board.
//!   4. Watch the instruction stream until the case finishes (`$8000` /
//!      `$E16F`), then read the verdict from the captured text: a pass prints
//!      " - OK", a fail prints a register/memory mismatch. See the trap-address
//!      note below for why the exit address alone can't tell them apart.
//!
//! `#[ignore]`'d — needs local C64 ROMs at `~/.emu198x/roms/commodore-c64/`
//! and the Lorenz suite (set `EMU198X_6502_LORENZ_DIR`, or drop it under
//! `198x/assets/test-suites/vice/bin`).

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use machine_commodore_c64::{C64, C64Config, C64Model};

// ---- Trap addresses -------------------------------------------------------
//
// A Lorenz case (`template.asm` in the suite) prints its name, runs `main`,
// then either prints " - OK" (pass) or a register/memory mismatch (fail); it
// finally calls `waitkey` and jumps to `$8000`. Because our GETIN stub reports
// the STOP key, *both* the pass and fail paths funnel through `waitkey` to
// `$8000` — so the exit address alone can't tell them apart. The verdict lives
// in the printed text: **a pass prints " - OK", a fail prints a mismatch and
// never does.** So `$8000` / `$E16F` mean "the case finished"; the pass/fail
// call is [`Outcome::passed`], read from the captured CHROUT text. `$A474`
// (BASIC error re-entry), a wild `$0000` fetch, and a processor jam are
// "didn't finish cleanly".

/// The case reached its end and called `waitkey` (see above).
const TRAP_FINISHED_1: u16 = 0x8000;
const TRAP_FINISHED_2: u16 = 0xE16F;
/// The BASIC error re-entry — the case derailed before finishing.
const TRAP_DERAILED: u16 = 0xA474;
/// The Lorenz "- OK" pass marker, as captured through CHROUT (our `petscii`
/// upper-cases the lowercase PETSCII the test emits).
const PASS_MARKER: &str = "- OK";

/// KERNAL RAM indirect vectors we re-point at RAM stubs.
const VEC_CHROUT: u16 = 0x0326; // JMP ($0326) from $FFD2
const VEC_STOP: u16 = 0x0328; // JMP ($0328) from $FFE1
const VEC_GETIN: u16 = 0x032A; // JMP ($032A) from $FFE4

/// Tiny RTS-terminated stubs, parked in the cassette buffer ($033C-…), which
/// the Lorenz cases don't use.
const STUB_CHROUT: u16 = 0x033C; // RTS — the harness reads A on entry
const STUB_GETIN: u16 = 0x0340; // LDA #$03 : RTS — report the STOP key
const STUB_STOP: u16 = 0x0344; // LDA #$FF : RTS — report "STOP not pressed"

const TEST_ENTRY: u16 = 0x0801;

const DEFAULT_SAFETY_CYCLE_BUDGET: u64 = 40_000_000;

/// The Lorenz cases the CPU-only harness lists as hardware-dependent and skips
/// (`KNOWN_HARDWARE_DEPENDENT` in `mos-6502/tests/lorenz_tests.rs`). This is the
/// set this machine-level harness exists to score.
const HARDWARE_DEPENDENT: &[&str] = &[
    // CIA timer A / B internals + interaction.
    "cia1ta",
    "cia1tab",
    "cia1tb",
    "cia1tb123",
    "cia2ta",
    "cia2tb",
    "cia2tb123",
    // CPU-side IRQ / NMI gating, NMI-over-IRQ priority.
    "irq",
    "nmi",
    // CPU bus timing the CPU-only harness can't measure.
    "cputiming",
    // 6510 MMU / banking — needs the real PLA, not a flat map.
    "mmu",
    "mmufetch",
    // The last two KERNAL load-trap variants exercise tape-side timing.
    "trap16",
    "trap17",
    // Final synthesiser that drives the KERNAL screen-clear.
    "finish",
];

/// Cases that pass on the machine *today* (verified locally against the real
/// ROMs + suite). The regression gate asserts these keep printing " - OK".
///
/// The rest of [`HARDWARE_DEPENDENT`] stay red and are tracked, not asserted:
///   - the seven CIA timer cases + `irq` + `nmi` print a one-cycle-off
///     register mismatch — they need the CIA cycle-delay pipeline (#17);
///   - `mmu` prints a banking mismatch (`$01` read-back), a genuine PLA gap
///     distinct from #17 (its sibling `mmufetch` already passes);
///   - `trap16` / `trap17` (tape KERNAL load traps) and `finish` (the suite
///     finaliser) need tape-side state this standalone harness doesn't set up.
const EXPECTED_PASS: &[&str] = &["cputiming", "mmufetch"];

// ---- Harness --------------------------------------------------------------

struct Outcome {
    passed: bool,
    cycles: u64,
    /// Everything the case printed through CHROUT — its name, then either the
    /// " - OK" pass marker or the failing register/memory dump.
    output: String,
}

/// Runs one Lorenz case against a freshly-cloned copy of the booted machine.
fn run_case(booted: &C64, program: &[u8]) -> Result<Outcome, String> {
    if program.len() < 2 {
        return Err("Lorenz test file too short for a load address".to_owned());
    }
    let load_addr = u16::from_le_bytes([program[0], program[1]]);
    let body = &program[2..];

    let mut machine = booted.clone();

    // Load the test image into RAM.
    for (i, &byte) in body.iter().enumerate() {
        let addr = load_addr.wrapping_add(i as u16);
        machine.poke(addr, byte);
    }

    // Park the RTS stubs and re-point the KERNAL RAM I/O vectors at them.
    machine.poke(STUB_CHROUT, 0x60); // RTS
    machine.poke(STUB_GETIN, 0xA9); // LDA #$03 — the CPU-only harness returns
    machine.poke(STUB_GETIN + 1, 0x03); // this from scan-keyboard so the Lorenz
    machine.poke(STUB_GETIN + 2, 0x60); // RTS  cases advance past their key wait.
    machine.poke(STUB_STOP, 0xA9); // LDA #$FF
    machine.poke(STUB_STOP + 1, 0xFF);
    machine.poke(STUB_STOP + 2, 0x60); // RTS
    write_vector(&mut machine, VEC_CHROUT, STUB_CHROUT);
    write_vector(&mut machine, VEC_GETIN, STUB_GETIN);
    write_vector(&mut machine, VEC_STOP, STUB_STOP);

    // Seed the CPU straight to the test entry at a clean instruction boundary.
    seed_entry(&mut machine, TEST_ENTRY);

    let mut output: Vec<u8> = Vec::new();
    let start_cycles = machine.cpu().total_cycles;
    let mut last_opcode_addr = TEST_ENTRY;

    loop {
        // Observe the instruction stream at each opcode fetch (SYNC high on a
        // read). We check before stepping past the fetch so a terminal trap
        // stops the run without executing the trap body.
        if machine.cpu().sync && machine.cpu().rw {
            let pc = machine.cpu().addr;
            last_opcode_addr = pc;
            match pc {
                STUB_CHROUT => output.push(machine.cpu().regs.a),
                TRAP_FINISHED_1 | TRAP_FINISHED_2 => {
                    let text = petscii(&output);
                    return Ok(Outcome {
                        passed: text.contains(PASS_MARKER),
                        cycles: machine.cpu().total_cycles - start_cycles,
                        output: text,
                    });
                }
                TRAP_DERAILED => {
                    return Err(format!(
                        "derailed to BASIC error re-entry $A474; output: {}",
                        petscii(&output)
                    ));
                }
                0x0000 => {
                    return Err(format!("wild fetch at $0000; output: {}", petscii(&output)));
                }
                _ => {}
            }
        }

        if machine.cpu().total_cycles - start_cycles > DEFAULT_SAFETY_CYCLE_BUDGET {
            return Err(format!(
                "exceeded {DEFAULT_SAFETY_CYCLE_BUDGET}-cycle budget; last opcode ${last_opcode_addr:04X}; output: {}",
                petscii(&output)
            ));
        }
        if machine.cpu().halted {
            return Err(format!(
                "processor jammed at ${last_opcode_addr:04X}; output: {}",
                petscii(&output)
            ));
        }

        machine.tick();
    }
}

/// Write a little-endian 16-bit vector into RAM.
fn write_vector(machine: &mut C64, addr: u16, value: u16) {
    let [lo, hi] = value.to_le_bytes();
    machine.poke(addr, lo);
    machine.poke(addr + 1, hi);
}

/// Advance to a clean instruction boundary, then redirect the CPU to `entry`
/// with the register state the Lorenz suite expects (clean A/X/Y, SP=$FD,
/// I set). The board banking ($01=$37) is already correct from boot.
fn seed_entry(machine: &mut C64, entry: u16) {
    for _ in 0..32 {
        if machine.cpu().instruction_complete() && machine.cpu().sync {
            break;
        }
        machine.tick();
    }
    let cpu = machine.cpu_mut();
    cpu.regs.pc = entry;
    cpu.addr = entry;
    cpu.regs.sp = 0xFD;
    cpu.regs.p = 0x24; // I set, unused bit 5 set (the two always-on states)
    cpu.regs.a = 0;
    cpu.regs.x = 0;
    cpu.regs.y = 0;
}

/// Boot a real C64 to the BASIC `READY.` prompt.
fn boot_to_ready(machine: &mut C64) -> Result<(), String> {
    // R E A D Y in screen codes.
    const READY: [u8; 5] = [0x12, 0x05, 0x01, 0x04, 0x19];
    const CAP: u64 = 6_000_000;
    while machine.cpu().total_cycles < CAP {
        for _ in 0..50_000 {
            machine.tick();
        }
        if screen_contains(machine, &READY) {
            return Ok(());
        }
    }
    Err("machine did not reach READY. within the boot budget".to_owned())
}

fn screen_contains(machine: &C64, needle: &[u8]) -> bool {
    let mut row = [0u8; 40 * 25];
    for (i, cell) in row.iter_mut().enumerate() {
        *cell = machine.peek(0x0400 + i as u16);
    }
    row.windows(needle.len()).any(|w| w == needle)
}

/// Collapse a multi-line CHROUT dump to one truncated line for the ledger.
fn one_line(text: &str) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    flat.chars().take(96).collect()
}

fn petscii(bytes: &[u8]) -> String {
    let mut out = String::new();
    for &b in bytes {
        match b {
            0x0D => out.push('\n'),
            0x20..=0x5A => out.push(b as char),
            0x61..=0x7A => out.push((b - 0x20) as char),
            _ => {
                let _ = write!(out, "<{b:02X}>");
            }
        }
    }
    out
}

// ---- Fixtures -------------------------------------------------------------

fn rom_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").expect("HOME should be set"))
        .join(".emu198x/roms/commodore-c64")
}

fn read_rom(name: &str) -> Vec<u8> {
    let path = rom_dir().join(name);
    fs::read(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

fn lorenz_dir() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(dir) = std::env::var_os("EMU198X_6502_LORENZ_DIR") {
        candidates.push(PathBuf::from(dir));
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/runtime-commodore-c64 → repo → 198x
    candidates.push(manifest.join("../../../assets/test-suites/vice/bin"));
    candidates.push(manifest.join("../../test-data/commodore/c64/lorenz"));
    candidates.into_iter().find(|p| p.join("irq").is_file())
}

fn booted_machine() -> C64 {
    let kernal = read_rom("kernal.rom");
    let basic = read_rom("basic.rom");
    let chargen = read_rom("chargen.rom");
    let mut machine = C64::new(C64Config {
        model: C64Model::PalBreadbin,
        kernal_rom: &kernal,
        basic_rom: &basic,
        character_rom: &chargen,
    })
    .expect("real C64 ROMs should construct a machine");
    boot_to_ready(&mut machine).expect("C64 should boot to READY.");
    machine
}

// ---- Tests ----------------------------------------------------------------

#[test]
fn expected_pass_is_a_subset_of_the_hardware_dependent_set() {
    // Guards against a typo in EXPECTED_PASS drifting from HARDWARE_DEPENDENT.
    for name in EXPECTED_PASS {
        assert!(
            HARDWARE_DEPENDENT.contains(name),
            "{name} is in EXPECTED_PASS but not HARDWARE_DEPENDENT"
        );
    }
}

/// Ledger: run every hardware-dependent Lorenz case against the real machine,
/// print the pass/fail table, and assert the currently-passing set does not
/// regress. Cases still blocked on the CIA cycle-delay pipeline (#17) are
/// reported, not asserted.
#[test]
#[ignore = "requires local C64 ROMs + the Wolfgang Lorenz suite"]
fn lorenz_machine_hardware_dependent_ledger() {
    let Some(dir) = lorenz_dir() else {
        panic!("Lorenz suite not found; set EMU198X_6502_LORENZ_DIR");
    };
    let booted = booted_machine();

    let mut passed = Vec::new();
    let mut failed = Vec::new();

    println!("=== Full-machine Lorenz ledger ===");
    println!("Suite: {}", dir.display());
    for &name in HARDWARE_DEPENDENT {
        let program = match fs::read(dir.join(name)) {
            Ok(bytes) => bytes,
            Err(e) => {
                println!("  MISS  {name:<12} ({e})");
                failed.push(name);
                continue;
            }
        };
        match run_case(&booted, &program) {
            Ok(outcome) if outcome.passed => {
                println!("  PASS  {name:<12} ({} cycles)", outcome.cycles);
                passed.push(name);
            }
            Ok(outcome) => {
                let dump = one_line(&outcome.output);
                println!("  FAIL  {name:<12} (mismatch) {dump}");
                failed.push(name);
            }
            Err(message) => {
                let first: String = message
                    .lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(88)
                    .collect();
                println!("  FAIL  {name:<12} — {first}");
                failed.push(name);
            }
        }
    }

    println!(
        "\nLorenz machine ledger: {}/{} passed",
        passed.len(),
        HARDWARE_DEPENDENT.len()
    );
    println!("  still red (tracked — CIA pipeline #17, mmu banking, tape traps): {failed:?}");

    // Regression gate: every case we expect to pass today must still pass.
    let regressions: Vec<&str> = EXPECTED_PASS
        .iter()
        .copied()
        .filter(|name| !passed.contains(name))
        .collect();
    assert!(
        regressions.is_empty(),
        "cases that used to pass now fail: {regressions:?}"
    );
}
