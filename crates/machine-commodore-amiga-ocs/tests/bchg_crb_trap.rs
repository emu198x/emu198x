//! Check whether the CPU ever executes the BCHG #0, CRB instruction
//! at $FE94BC that would toggle the Timer B START bit.
//!
//! If this fires, Timer B gets started. If not, something upstream
//! is skipping the call. We also need to verify BCHG is actually
//! performing the R-M-W write when it runs.

use std::path::PathBuf;
use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};

const PC_BCHG: u32 = 0x00FE_94BC;
const PC_AFTER_BCHG: u32 = 0x00FE_94C2;

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        eprintln!("skipping: Kickstart 1.3 ROM missing at {}", path.display());
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

#[test]
#[ignore]
fn trap_bchg_crb() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);

    let mut bchg_hits = 0u64;
    let mut after_hits = 0u64;
    let mut prev_pc = amiga.cpu().regs.pc;
    let mut first_hit_tick: Option<u64> = None;
    let mut tick = 0u64;

    for _ in 0..(400 * PAL_FRAME_TICKS) {
        amiga.tick();
        tick += 1;
        let pc = amiga.cpu().regs.pc;
        if pc == prev_pc {
            continue;
        }
        if pc == PC_BCHG {
            bchg_hits += 1;
            if first_hit_tick.is_none() {
                first_hit_tick = Some(tick);
            }
        }
        if pc == PC_AFTER_BCHG {
            after_hits += 1;
        }
        prev_pc = pc;
    }

    eprintln!("\n=== BCHG #0, CRB at $FE94BC ===");
    eprintln!("BCHG hits:          {bchg_hits}");
    eprintln!("After-BCHG hits:    {after_hits}");
    if let Some(t) = first_hit_tick {
        let cck = t / 2;
        let frame = cck / 70824;
        eprintln!("First hit at tick {t} (frame ~{frame})");
    }
    if bchg_hits == 0 {
        eprintln!("\n→ BCHG never executes. Upstream caller doesn't reach it.");
    } else {
        eprintln!("\n→ BCHG executes. Check if our emulator's R-M-W does the CRB write.");
    }

    // Also check every CIA-A write — filter for CRB specifically.
    let crb_writes: Vec<_> = amiga
        .debug_cia_a_cr_log
        .iter()
        .filter(|(_, _, reg, _)| *reg == 0xF)
        .collect();
    eprintln!("\nCIA-A CRB writes total: {}", crb_writes.len());
    for (cck, pc, _, val) in &crb_writes {
        let frame = cck / 70824;
        eprintln!(
            "  frame~{frame:<3}  pc=${pc:08X}  CRB=${val:02X}"
        );
    }
}
