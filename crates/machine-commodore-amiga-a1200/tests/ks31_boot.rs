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

    // Sample PC + IPL + VBR at every checkpoint. KS 3.x lowers the
    // CPU IPL mask once init reaches the "interrupts on" phase and
    // moves VBR to its chip-RAM exception table. Those transitions
    // are the most informative progress signals.
    // Stage F: back to 5000 frames now that we've confirmed (via the
    // 50K-frame run) KS never breaks out of the Wack loop naturally.
    // 5000 is plenty to dump the chip-RAM vector table KS installed.
    let frames_to_run: u64 = 5_000;
    let checkpoint_every: u64 = 500;
    let mut last_checkpoint_pc = initial_pc;
    let mut min_ipl_seen = 7u8;
    let mut first_vbr_change_frame: Option<u64> = None;
    let mut first_ipl_drop_frame: Option<u64> = None;
    // Exception tracking: count None -> Some(vector) transitions on
    // the cpu.exc_vector field. The field stays Some for the duration
    // of exception processing (multiple ticks), so the edge tells us
    // when a *new* exception was taken. PC-at-edge gives the address
    // that triggered.
    let mut exc_counts: std::collections::HashMap<u8, u64> = std::collections::HashMap::new();
    let mut exc_first_pc: std::collections::HashMap<u8, u32> = std::collections::HashMap::new();
    let mut prev_exc: Option<u8> = m.cpu().exc_vector;
    // Hot PCs: sample PC at every 128th tick (~7M samples over 5000
    // PAL frames). The hottest PCs reveal which loops are eating
    // emulated time.
    let mut hot_pcs: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
    let mut tick_counter: u64 = 0;
    // Track entries into $F83182 (the byte-receive routine that leads
    // to the Wack loop). On the rising edge of PC == $F83182, read
    // the return address from the supervisor stack to identify the
    // caller.
    let mut prev_pc = m.cpu().regs.pc;
    let mut byte_receive_entries: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
    for f in 1..=frames_to_run {
        for _ in 0..PAL_FRAME_TICKS {
            m.tick();
            tick_counter = tick_counter.wrapping_add(1);
            let pc = m.cpu().regs.pc;
            unique_pcs.insert(pc);
            if tick_counter & 0x7F == 0 {
                *hot_pcs.entry(pc).or_insert(0) += 1;
            }
            if (0x00F8_0000..0x0100_0000).contains(&pc) {
                last_pc_in_rom = pc;
            } else if pc < 0x00F8_0000 {
                excursion_count += 1;
            }
            // Track entries into the byte-receive routine. Key by the
            // *previous* PC — the BSR instruction site that branched
            // here. Stack pointer reads are unreliable mid-tick.
            if pc == 0x00F8_3182 && prev_pc != 0x00F8_3182 {
                *byte_receive_entries.entry(prev_pc).or_insert(0) += 1;
            }
            prev_pc = pc;
            let ipl = m.cpu().regs.interrupt_mask();
            if ipl < min_ipl_seen {
                min_ipl_seen = ipl;
                if first_ipl_drop_frame.is_none() {
                    first_ipl_drop_frame = Some(f);
                }
            }
            if first_vbr_change_frame.is_none() && m.cpu().regs.vbr != 0 {
                first_vbr_change_frame = Some(f);
            }
            // Exception-vector edge detection.
            let cur_exc = m.cpu().exc_vector;
            if cur_exc != prev_exc && cur_exc.is_some() {
                let v = cur_exc.unwrap();
                *exc_counts.entry(v).or_insert(0) += 1;
                exc_first_pc.entry(v).or_insert(m.cpu().instr_start_pc);
            }
            prev_exc = cur_exc;
        }
        if f % checkpoint_every == 0 {
            let cpu = m.cpu();
            eprintln!(
                "  checkpoint frame {f:>4}:  PC=${:08X}  IPL={}  VBR=${:08X}  custom_writes={}  intena_writes={}",
                cpu.regs.pc,
                cpu.regs.interrupt_mask(),
                cpu.regs.vbr,
                m.debug_custom_write_log.len(),
                m.debug_intena_writes,
            );
            last_checkpoint_pc = cpu.regs.pc;
        }
    }
    eprintln!(
        "milestones:  min IPL = {min_ipl_seen}  first IPL drop = {:?}  first VBR change = {:?}",
        first_ipl_drop_frame, first_vbr_change_frame
    );

    // Hot PCs — where is the CPU actually spending most of its time?
    let mut hot_sorted: Vec<_> = hot_pcs.iter().collect();
    hot_sorted.sort_by(|a, b| b.1.cmp(a.1));
    let total_samples: u64 = hot_sorted.iter().map(|(_, c)| **c).sum();
    eprintln!(
        "hot PCs (top 10, sampled every 128th tick, {} total samples):",
        total_samples
    );
    for (pc, count) in hot_sorted.iter().take(10) {
        let pct = (**count as f64 / total_samples as f64) * 100.0;
        eprintln!("  ${pc:08X}: {count} samples ({pct:.1}%)");
    }

    // Byte-receive call sites (sorted by frequency).
    let mut br_sorted: Vec<_> = byte_receive_entries.iter().collect();
    br_sorted.sort_by(|a, b| b.1.cmp(a.1));
    let total_br: u64 = byte_receive_entries.values().sum();
    eprintln!("byte-receive $F83182 entries (total {total_br}, keyed by BSR site PC):");
    for (caller_pc, count) in br_sorted.iter().take(10) {
        eprintln!("  from ${caller_pc:08X}: {count} entries");
    }

    // Exception counts — if KS is hitting an illegal-instruction trap
    // or line-A/F trap and falling into a reset handler, these counts
    // will be high.
    let mut exc_sorted: Vec<_> = exc_counts.iter().collect();
    exc_sorted.sort_by(|a, b| b.1.cmp(a.1));
    eprintln!("exceptions taken (top 10):");
    if exc_sorted.is_empty() {
        eprintln!("  (none)");
    }
    for (vector, count) in exc_sorted.iter().take(10) {
        let first_pc = exc_first_pc.get(vector).copied().unwrap_or(0);
        eprintln!(
            "  vector {:>3} ({}): {count} taken, first at PC=${first_pc:08X}",
            vector,
            exception_vector_name(**vector)
        );
    }

    // Hottest custom-register writes — keyed by *offset* (the
    // `debug_custom_write_log` tuple stores PC at .1 and chipset
    // offset at .3, so group by .3).
    let mut writes_by_offset: std::collections::HashMap<u16, u64> = std::collections::HashMap::new();
    for entry in m.debug_custom_write_log.iter() {
        *writes_by_offset.entry(entry.3).or_insert(0) += 1;
    }
    let mut sorted: Vec<_> = writes_by_offset.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    eprintln!("hottest custom register writes (top 5, by chipset offset):");
    for (offset, count) in sorted.iter().take(5) {
        eprintln!(
            "  $DFF{:03X} ({}): {count} writes",
            offset,
            custom_register_name(*offset)
        );
    }

    // Hottest custom-register reads — keyed by chipset offset.
    let mut reads_sorted: Vec<_> = m.debug_reg_read_counts.iter().collect();
    reads_sorted.sort_by(|a, b| b.1.cmp(a.1));
    eprintln!("hottest custom register reads (top 5, by chipset offset):");
    for (offset, count) in reads_sorted.iter().take(5) {
        eprintln!(
            "  $DFF{:03X} ({}): {count} reads",
            offset,
            custom_register_name(**offset)
        );
    }

    report_state(&format!("after {frames_to_run} frames"), &m, frames_to_run);
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

    // Vector table inspection. KS sets up the 68k exception vector
    // table at low chip-RAM addresses ($00000000-$000003FF). If KS
    // has cleared OVL, the CPU reads these from RAM directly. If OVL
    // is still set, vectors are read from the ROM mirror.
    eprintln!("OVL state at end of run: {}", m.memory().overlay());
    eprintln!("Chip-RAM exception vector table after boot run:");
    let mem = m.memory();
    for vec in [0u32, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 24, 31] {
        let off = (vec * 4) as u32;
        // Read directly from chip RAM (bypassing OVL).
        let hi = mem.read_chip_ram_word(off);
        let lo = mem.read_chip_ram_word(off + 2);
        let val = (u32::from(hi) << 16) | u32::from(lo);
        eprintln!("  vec {vec:>2} @ chip[${off:08X}]: ${val:08X}");
    }
}

/// Human-readable name for a 68k exception vector.
fn exception_vector_name(vector: u8) -> &'static str {
    match vector {
        0 => "reset SSP",
        1 => "reset PC",
        2 => "bus error",
        3 => "address error",
        4 => "illegal instruction",
        5 => "divide by zero",
        6 => "CHK / CHK2",
        7 => "TRAPV / TRAPcc",
        8 => "privilege violation",
        9 => "trace",
        10 => "line A (Axxx)",
        11 => "line F (Fxxx)",
        14 => "format error (68010+ RTE)",
        24 => "spurious interrupt",
        25..=31 => "autovector IRQ",
        32..=47 => "TRAP #n",
        _ => "user/MFP/other",
    }
}

/// Human-readable name for a chipset register offset (the names KS
/// authors used in the Hardware Reference Manual).
fn custom_register_name(offset: u16) -> &'static str {
    match offset {
        0x002 => "DMACONR",
        0x004 => "VPOSR",
        0x006 => "VHPOSR",
        0x00A => "JOY0DAT",
        0x00C => "JOY1DAT",
        0x010 => "ADKCONR",
        0x012 => "POT0DAT",
        0x014 => "POT1DAT",
        0x016 => "POTGOR",
        0x018 => "SERDATR",
        0x01A => "DSKBYTR",
        0x01C => "INTENAR",
        0x01E => "INTREQR",
        0x07E => "DSKSYNC",
        0x09A => "INTENA",
        0x09C => "INTREQ",
        0x09E => "ADKCON",
        0x180 => "COLOR00",
        _ => "?",
    }
}
