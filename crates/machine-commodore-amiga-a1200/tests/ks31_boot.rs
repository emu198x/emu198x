//! Stage C of the A1200 rollout (see
//! `knowledge/decisions/amiga-machine-rollout-plan.md`).
//!
//! Loads the real Kickstart 3.1 ROM (Cloanto / Hyperion-licensed, user-
//! supplied) into the A1200 machine with `Cpu68020` swapped in, runs N
//! frames, and reports where the boot stops, hangs, or faults. The
//! deliverable is the *first observed failure* — Stage D plans the
//! fix from whatever this test surfaces.
//!
//! ROM lookup order:
//! 1. `$EMU198X_KS31_A1200_ROM` env var (explicit path).
//! 2. `~/.emu198x/roms/commodore-amiga/kick31a1200.rom` (default).
//!
//! If neither resolves the test skips loudly with `eprintln!` rather
//! than failing — KS 3.1 is not redistributable and CI machines
//! without the user's licensed copy should still pass the suite.

use machine_commodore_amiga_a1200::{AmigaA1200, PAL_FRAME_TICKS, RamConfig};
use std::path::PathBuf;

fn load_ks31_rom() -> Option<Vec<u8>> {
    let path = match std::env::var("EMU198X_KS31_A1200_ROM") {
        Ok(p) => PathBuf::from(p),
        Err(_) => {
            let home = std::env::var("HOME").expect("HOME is set");
            PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick31a1200.rom")
        }
    };
    if !path.exists() {
        eprintln!(
            "skipping: KS 3.1 A1200 ROM missing at {} (set $EMU198X_KS31_A1200_ROM to override)",
            path.display()
        );
        return None;
    }
    let bytes = std::fs::read(&path).expect("read KS 3.1 ROM");
    eprintln!("loaded KS 3.1 A1200 ROM: {} bytes from {}", bytes.len(), path.display());
    Some(bytes)
}

fn a1200_2mb_chip(rom: Vec<u8>) -> AmigaA1200 {
    AmigaA1200::with_ram_config(
        rom,
        RamConfig {
            chip_kb: 2048,
            slow_kb: 0,
            fast_kb: 0,
        },
    )
}

/// Run for `frames` PAL frames and report the CPU state, focusing on
/// what's visible at the failure boundary.
fn report_state(label: &str, m: &AmigaA1200, frames: u64) {
    let cpu = m.cpu();
    eprintln!("--- {label} after {frames} frames ---");
    eprintln!("  PC = ${:08X}", cpu.regs.pc);
    eprintln!("  SR = ${:04X} ({}supervisor, IPL mask {})",
        cpu.regs.sr,
        if cpu.regs.is_supervisor() { "" } else { "user — NOT " },
        cpu.regs.interrupt_mask());
    eprintln!("  USP=${:08X} SSP=${:08X}", cpu.regs.usp, cpu.regs.ssp);
    eprintln!(
        "  D0..D7 = {}",
        (0..8)
            .map(|i| format!("${:08X}", cpu.regs.d[i]))
            .collect::<Vec<_>>()
            .join(" ")
    );
    eprintln!(
        "  A0..A6 = {} A7=${:08X} (active SP)",
        (0..7)
            .map(|i| format!("${:08X}", cpu.regs.a[i]))
            .collect::<Vec<_>>()
            .join(" "),
        if cpu.regs.is_supervisor() {
            cpu.regs.ssp
        } else {
            cpu.regs.usp
        }
    );
    eprintln!(
        "  VBR=${:08X} SFC={} DFC={}",
        cpu.regs.vbr, cpu.regs.sfc, cpu.regs.dfc
    );
}

/// Dump the next ~16 bytes of code starting at `pc`, formatted as a
/// run of words for manual disassembly.
fn dump_code_at(m: &AmigaA1200, pc: u32, words: u32) {
    eprintln!("  code @ ${pc:08X}:");
    eprint!("   ");
    for i in 0..words {
        let w = m.read_word(pc.wrapping_add(i * 2));
        eprint!(" {:04X}", w);
    }
    eprintln!();
}

#[test]
fn ks31_boots_far_enough_to_advance_pc_past_reset_vector() {
    let Some(rom) = load_ks31_rom() else { return };

    let mut m = a1200_2mb_chip(rom);

    let initial_pc = m.cpu().regs.pc;
    eprintln!("initial PC after reset_to: ${initial_pc:08X}");
    assert_ne!(initial_pc, 0, "PC should not be zero after reset_to");
    assert!(
        (0x00F8_0000..0x0100_0000).contains(&initial_pc),
        "initial PC ${initial_pc:08X} should sit in the ROM window $F80000-$FFFFFF"
    );

    // Track unique PCs visited over the run — a tight loop will show
    // a small number despite many ticks; healthy boot shows hundreds
    // or thousands.
    let mut unique_pcs = std::collections::BTreeSet::new();
    let mut last_pc_in_rom: u32 = initial_pc;
    let mut excursion_count: u64 = 0;

    let frames_to_run: u64 = 50;
    for _ in 0..(frames_to_run * PAL_FRAME_TICKS) {
        m.tick();
        let pc = m.cpu().regs.pc;
        unique_pcs.insert(pc);
        if (0x00F8_0000..0x0100_0000).contains(&pc) {
            last_pc_in_rom = pc;
        } else if pc < 0x00F8_0000 {
            excursion_count += 1;
        }
    }

    report_state("after 50 frames (~1s PAL)", &m, frames_to_run);
    eprintln!(
        "unique PCs visited: {}   last PC in ROM: ${:08X}   excursions out of ROM: {}",
        unique_pcs.len(),
        last_pc_in_rom,
        excursion_count
    );
    dump_code_at(&m, m.cpu().regs.pc, 8);

    // Chipset activity counters — proxy for "did the boot touch
    // hardware at all?"
    eprintln!(
        "chipset activity:  custom_write_log={}   intena_writes={}   reg_read_kinds={}",
        m.debug_custom_write_log.len(),
        m.debug_intena_writes,
        m.debug_reg_read_counts.len()
    );

    eprintln!(
        "PC delta from initial: ${:08X} -> ${:08X}  ({} unique addresses seen)",
        initial_pc,
        m.cpu().regs.pc,
        unique_pcs.len()
    );
}
