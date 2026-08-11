//! nestest.nes validation against the golden log.
//!
//! nestest is Kevin Horton's CPU instruction exerciser for the NES.
//! It tests every documented 6502 opcode (and many undocumented ones)
//! by running them with known inputs and checking the results.
//!
//! The golden log (`nestest.log`) records the CPU state at every
//! instruction fetch — PC, A, X, Y, P, SP, plus PPU dot/scanline and
//! total CPU cycle count. This test loads the ROM, forces PC to $C000
//! (the automated test entry point, not the visual-mode entry at
//! $C004 which the reset vector points to), and compares register
//! state at every instruction boundary against the log.
//!
//! # Fixture location
//!
//! The test needs two files:
//! - `nestest.nes` — the ROM (NROM, mapper 0, 16 KiB PRG + 8 KiB CHR)
//! - `nestest.log` — the golden reference (8,991 lines)
//!
//! Resolved in order:
//! 1. `NES_TEST_DATA` environment variable (directory containing both)
//! 2. `~/Projects/198x/assets/nintendo/nes/test-suites/other/`
//! 3. `~/Projects/198x/assets/test-suites/nes-test-roms/other/`
//!
//! If none resolves, the test is a no-op.

use machine_nintendo_nes::Nes;
use std::path::PathBuf;

fn fixture_dir() -> Option<PathBuf> {
    let has_fixture =
        |d: &PathBuf| d.join("nestest.nes").exists() && d.join("nestest.log").exists();

    if let Ok(p) = std::env::var("NES_TEST_DATA") {
        let d = PathBuf::from(p);
        if has_fixture(&d) {
            return Some(d);
        }
    }
    let home = std::env::var_os("HOME")?;
    for rel in [
        "Projects/198x/assets/nintendo/nes/test-suites/other",
        "Projects/198x/assets/test-suites/nes-test-roms/other",
        "Projects/Emu198x-Unclean/Reference/nintendo/nes/test-suites/other",
    ] {
        let d = PathBuf::from(&home).join(rel);
        if has_fixture(&d) {
            return Some(d);
        }
    }
    None
}

/// Parse one line of the nestest golden log and extract
/// (PC, A, X, Y, P, SP, CYC).
fn parse_log_line(line: &str) -> Option<(u16, u8, u8, u8, u8, u8, u64)> {
    // Format: "C000  4C F5 C5  JMP $C5F5                       A:00 X:00 Y:00 P:24 SP:FD PPU:  0, 21 CYC:7"
    if line.len() < 73 {
        return None;
    }
    let pc = u16::from_str_radix(&line[0..4], 16).ok()?;

    // Find register fields by their labels.
    let a_pos = line.find("A:")?;
    let a = u8::from_str_radix(&line[a_pos + 2..a_pos + 4], 16).ok()?;

    let x_pos = line.find("X:")?;
    let x = u8::from_str_radix(&line[x_pos + 2..x_pos + 4], 16).ok()?;

    let y_pos = line.find("Y:")?;
    let y = u8::from_str_radix(&line[y_pos + 2..y_pos + 4], 16).ok()?;

    let p_pos = line.find("P:")?;
    let p = u8::from_str_radix(&line[p_pos + 2..p_pos + 4], 16).ok()?;

    let sp_pos = line.find("SP:")?;
    let sp = u8::from_str_radix(&line[sp_pos + 3..sp_pos + 5], 16).ok()?;

    let cyc_pos = line.find("CYC:")?;
    let cyc_str = &line[cyc_pos + 4..].trim();
    let cyc = cyc_str.parse::<u64>().ok()?;

    Some((pc, a, x, y, p, sp, cyc))
}

/// Quick smoke test: run the first 100 instructions and compare.
#[test]
fn nestest_smoke() {
    let Some(dir) = fixture_dir() else {
        emu198x_test_skip::skip!("nestest fixture not found");
    };

    let rom_data = std::fs::read(dir.join("nestest.nes")).expect("read nestest.nes");
    let log_data = std::fs::read_to_string(dir.join("nestest.log")).expect("read nestest.log");

    let parsed = format_nintendo_nes_ines::parse_ines(&rom_data).expect("parse nestest.nes");
    let mut nes = Nes::new(parsed.mapper);

    // Run through the 7-cycle reset bootstrap (21 PPU dots).
    for _ in 0..21 {
        nes.tick();
    }

    // Force PC to $C000 (automated test entry, not $C004 visual mode)
    // and restore SP to the nestest-log starting value (reset
    // decrements SP by 3 for phantom pushes, but the reference log
    // was recorded starting from SP=FD).
    nes.cpu.regs.pc = 0xC000;
    nes.cpu.regs.sp = 0xFD;
    nes.cpu.addr = 0xC000;
    nes.cpu.rw = true;
    nes.cpu.sync = true;

    let log_lines: Vec<&str> = log_data.lines().collect();
    let max_lines = 100.min(log_lines.len());

    run_and_compare(&mut nes, &log_lines[..max_lines], max_lines);
}

/// Advance the NES by exactly one CPU instruction. Ticks until
/// `instruction_complete()` goes true after at least one CPU cycle.
fn run_one_instruction(nes: &mut Nes) {
    // The CPU is at instruction_complete() == true (about to fetch).
    // First, tick until the CPU is mid-instruction (not complete).
    let mut started = false;
    for _ in 0..900 {
        nes.tick();
        if !nes.cpu.instruction_complete() {
            started = true;
        }
        if started && nes.cpu.instruction_complete() {
            return;
        }
    }
    panic!(
        "CPU stuck — instruction did not complete within 300 CPU cycles (PC={:04X})",
        nes.cpu.regs.pc,
    );
}

fn run_and_compare(nes: &mut Nes, log_lines: &[&str], label_count: usize) {
    let mut mismatches = Vec::new();

    for (i, &line) in log_lines.iter().enumerate() {
        let Some((exp_pc, exp_a, exp_x, exp_y, exp_p, exp_sp, _exp_cyc)) = parse_log_line(line)
        else {
            continue;
        };

        let pc = nes.cpu.regs.pc;
        let a = nes.cpu.regs.a;
        let x = nes.cpu.regs.x;
        let y = nes.cpu.regs.y;
        let p = nes.cpu.regs.p;
        let sp = nes.cpu.regs.sp;

        if pc != exp_pc || a != exp_a || x != exp_x || y != exp_y || p != exp_p || sp != exp_sp {
            mismatches.push(format!(
                "line {}: expected PC={:04X} A={:02X} X={:02X} Y={:02X} P={:02X} SP={:02X}, \
                 got PC={:04X} A={:02X} X={:02X} Y={:02X} P={:02X} SP={:02X}",
                i + 1,
                exp_pc,
                exp_a,
                exp_x,
                exp_y,
                exp_p,
                exp_sp,
                pc,
                a,
                x,
                y,
                p,
                sp,
            ));
            if mismatches.len() >= 10 {
                break;
            }
        }

        run_one_instruction(nes);
    }

    if !mismatches.is_empty() {
        for m in &mismatches {
            eprintln!("{m}");
        }
        panic!(
            "{} mismatches in first {label_count} instructions",
            mismatches.len()
        );
    }
}

/// Full nestest validation — all 8,991 instructions.
///
/// Run explicitly:
/// ```sh
/// cargo test --release -p machine-nintendo-nes --test nestest run_all \
///     -- --ignored --nocapture
/// ```
#[test]
#[ignore = "needs nestest.nes + nestest.log — run with --ignored"]
fn run_all() {
    let Some(dir) = fixture_dir() else {
        panic!(
            "nestest fixture not found — set NES_TEST_DATA or place files at \
             ~/Projects/Emu198x-Unclean/Reference/nintendo/nes/test-suites/other/"
        );
    };

    let rom_data = std::fs::read(dir.join("nestest.nes")).expect("read nestest.nes");
    let log_data = std::fs::read_to_string(dir.join("nestest.log")).expect("read nestest.log");

    let parsed = format_nintendo_nes_ines::parse_ines(&rom_data).expect("parse nestest.nes");
    let mut nes = Nes::new(parsed.mapper);

    // Run the full 7-cycle reset bootstrap (21 PPU dots) so the reset
    // sequence completes before we override entry state — otherwise the
    // unfinished reset reloads PC from the reset vector and clobbers the
    // forced value.
    for _ in 0..21 {
        nes.tick();
    }

    // Force PC to $C000 (automated test entry, not the $C004 visual-mode
    // entry the reset vector points to) and restore SP to the
    // nestest-log starting value (reset decrements SP by 3 for phantom
    // pushes, but the reference log was recorded from SP=FD).
    nes.cpu.regs.pc = 0xC000;
    nes.cpu.regs.sp = 0xFD;
    nes.cpu.addr = 0xC000;
    nes.cpu.rw = true;
    nes.cpu.sync = true;

    let log_lines: Vec<&str> = log_data.lines().collect();
    let total = log_lines.len();

    eprintln!("Running {total} nestest instructions...");
    run_and_compare(&mut nes, &log_lines, total);

    // Check test result codes in memory.
    let result_official = nes.peek(0x02);
    let result_unofficial = nes.peek(0x03);
    eprintln!();
    eprintln!("=== nestest result codes ===");
    eprintln!("$02 (official opcodes):   0x{result_official:02X}");
    eprintln!("$03 (unofficial opcodes): 0x{result_unofficial:02X}");
    eprintln!("{total} / {total} instructions matched.");
}
