/// Klaus Dormann 6502 functional test harness.
///
/// Runs the prebuilt 64 KiB memory image and expects execution to settle into
/// the documented success self-loop at `$3469`.
///
/// Run the functional test:
/// `cargo test -p mos-6502 --test dormann_tests run_functional_test -- --ignored --nocapture`
mod support;

use std::path::{Path, PathBuf};

use mos_6502::M6502;
use support::find_dormann_6502_dir;

const FUNCTIONAL_BIN: &str = "bin_files/6502_functional_test.bin";
const FUNCTIONAL_START_PC: u16 = 0x0400;
const FUNCTIONAL_SUCCESS_PC: u16 = 0x3469;
const DEFAULT_SAFETY_CYCLE_BUDGET: u64 = 200_000_000;
const CYCLE_BUDGET_ENV: &str = "EMU198X_6502_DORMANN_CYCLE_BUDGET";

fn dormann_functional_bin_path(root: &Path) -> PathBuf {
    root.join(FUNCTIONAL_BIN)
}

fn load_full_memory_image(path: &Path) -> [u8; 65536] {
    let bytes = std::fs::read(path).expect("failed to read Dormann functional binary");
    let bytes: [u8; 65536] = bytes.try_into().unwrap_or_else(|_| {
        panic!(
            "expected 65536-byte Dormann memory image at {}",
            path.display()
        )
    });
    bytes
}

fn cycle_budget_from_env() -> Option<u64> {
    match std::env::var(CYCLE_BUDGET_ENV) {
        Ok(value) if !value.trim().is_empty() => Some(value.parse().unwrap_or_else(|error| {
            panic!("failed to parse {CYCLE_BUDGET_ENV}='{value}': {error}")
        })),
        _ => None,
    }
}

fn setup_cpu(mem: &[u8; 65536]) -> M6502 {
    let mut cpu = M6502::new();
    cpu.regs.pc = FUNCTIONAL_START_PC;
    cpu.total_cycles = 0;
    cpu.addr = FUNCTIONAL_START_PC;
    cpu.data = 0;
    cpu.rw = true;
    cpu.sync = true;
    cpu.data_in = mem[FUNCTIONAL_START_PC as usize];
    cpu.irq = false;
    cpu.nmi = false;
    cpu.rdy = true;
    cpu.halted = false;
    cpu
}

fn run_until_terminal(mem: &mut [u8; 65536], safety_cycle_budget: u64) -> Result<u64, String> {
    let mut cpu = setup_cpu(mem);
    let mut previous_completed_pc = None;

    while cpu.total_cycles < safety_cycle_budget {
        if cpu.rw {
            cpu.data_in = mem[cpu.addr as usize];
        } else {
            mem[cpu.addr as usize] = cpu.data;
        }

        let completed = cpu.tick();
        if !completed || !cpu.instruction_complete() {
            continue;
        }

        let completed_pc = cpu.regs.pc;
        if previous_completed_pc == Some(completed_pc) {
            if completed_pc == FUNCTIONAL_SUCCESS_PC {
                return Ok(cpu.total_cycles);
            }
            return Err(format!(
                "Dormann functional test trapped at ${completed_pc:04X} after {} cycles",
                cpu.total_cycles
            ));
        }
        previous_completed_pc = Some(completed_pc);
    }

    Err(format!(
        "Dormann functional test exceeded safety budget of {safety_cycle_budget} cycles; last PC at ${:04X}",
        previous_completed_pc.unwrap_or(cpu.regs.pc)
    ))
}

#[test]
fn dormann_functional_path_points_to_bin_file() {
    let path = dormann_functional_bin_path(Path::new("/tmp/dormann"));
    assert!(path.ends_with(FUNCTIONAL_BIN));
}

#[test]
#[ignore = "requires local Dormann 6502 functional suite"]
fn run_functional_test() {
    let root = find_dormann_6502_dir().expect("Dormann 6502 functional suite should exist");
    let bin = dormann_functional_bin_path(&root);
    let mut mem = load_full_memory_image(&bin);
    let cycles = run_until_terminal(
        &mut mem,
        cycle_budget_from_env().unwrap_or(DEFAULT_SAFETY_CYCLE_BUDGET),
    )
    .unwrap_or_else(|error| panic!("Dormann functional test failed: {error}"));

    println!(
        "Dormann 6502 functional test passed in {cycles} cycles at success PC ${FUNCTIONAL_SUCCESS_PC:04X}"
    );
}
