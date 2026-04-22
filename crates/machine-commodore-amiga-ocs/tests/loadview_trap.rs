//! Task #96 — does chip-only KS 1.3 boot ever call LoadView?
//!
//! LoadView lives at $FCD564 in KS 1.3. It takes a View pointer in
//! a1 and, when the View is non-null, copies `View->LOFCprList[1]`
//! into `GfxBase->LOFlist` (the write at $FCD5C2 we identified in
//! `loflist_write_hunt.rs`). That's the step slow-RAM performs at
//! frame 188 but chip-only never does.
//!
//! This test traps every PC hit on $FCD564 and records:
//!  - tick / frame
//!  - the View pointer passed in (from top of stack, i.e. 4(sp))
//!  - the return address (the caller's PC)
//!
//! If chip-only never hits $FCD564, the gap is upstream — we need
//! to find who SHOULD call LoadView in chip-only but doesn't.

use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};
use std::path::PathBuf;

const LOADVIEW_PC: u32 = 0x00FC_D564;

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

fn trap_loadview(label: &str, use_slow_ram: bool) {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = if use_slow_ram {
        AmigaOcs::with_slow_ram(rom, 512 * 1024)
    } else {
        AmigaOcs::new(rom)
    };

    eprintln!("\n########## {label} ##########");

    let mut hits = Vec::new();
    let mut prev_pc = amiga.cpu().regs.pc;
    let end = 700u64 * PAL_FRAME_TICKS as u64;
    for tick in 0..end {
        amiga.tick();
        let pc = amiga.cpu().regs.pc;
        if pc == prev_pc {
            continue;
        }
        if pc == LOADVIEW_PC {
            let regs = &amiga.cpu().regs;
            // SSP when supervisor mode (SR bit 13), USP otherwise.
            let sp = if regs.sr & 0x2000 != 0 {
                regs.ssp
            } else {
                regs.usp
            };
            let return_addr = amiga.read_long(sp);
            let view_ptr = amiga.read_long(sp.wrapping_add(4));
            let frame = tick / PAL_FRAME_TICKS as u64;
            hits.push((frame, tick, return_addr, view_ptr));
        }
        prev_pc = pc;
    }

    eprintln!("LoadView entries: {}", hits.len());
    for (frame, tick, ret, view) in hits.iter().take(20) {
        eprintln!("  frame~{frame:<3}  tick={tick:<10}  called from ${ret:08X}  View=${view:08X}");
    }
}

#[test]
#[ignore]
fn slow_ram_loadview_entries() {
    trap_loadview("slow-RAM", true);
}

#[test]
#[ignore]
fn chip_only_loadview_entries() {
    trap_loadview("chip-only", false);
}
