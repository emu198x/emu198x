//! Task #96 — inside LoadView, which branch does each config take?
//!
//! LoadView at $FCD564 has two branches after storing a1 into
//! GfxBase->ActiView:
//!   $FCD5B6  BEQ.S $FCD5D8   (a1 was NULL — use the dummy path:
//!                             LOFlist = copinit + $A0)
//!   $FCD5B8  (a1 non-null — the *real* path that reads
//!            View->LOFCprList and stores the buffer pointer into
//!            GfxBase->LOFlist at $FCD5C2)
//!
//! Slow-RAM takes the non-null branch at frame 188 (writing $B888).
//! Chip-only never reaches this function during the interesting
//! window, OR takes the NULL branch (which would still leave
//! LOFlist at copinit+$A0, not at ExecBase).
//!
//! We trap BOTH branches and for the non-null one, also capture
//! a1 (View ptr), 4(a1) (LOFCprList), and the value being written.

use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};
use std::path::PathBuf;

const PC_NONNULL_PATH: u32 = 0x00FC_D5B8;
const PC_NULL_PATH: u32 = 0x00FC_D5D8;
const PC_LOFLIST_WRITE: u32 = 0x00FC_D5C2;

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

fn trap(label: &str, use_slow_ram: bool) {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = if use_slow_ram {
        AmigaOcs::with_slow_ram(rom, 512 * 1024)
    } else {
        AmigaOcs::new(rom)
    };

    eprintln!("\n########## {label} ##########");
    let mut nonnull_hits = 0u64;
    let mut null_hits = 0u64;
    let mut writes = Vec::new();
    let mut prev_pc = amiga.cpu().regs.pc;
    let end = 700u64 * PAL_FRAME_TICKS;
    for tick in 0..end {
        amiga.tick();
        let pc = amiga.cpu().regs.pc;
        if pc == prev_pc {
            continue;
        }
        if pc == PC_NONNULL_PATH {
            nonnull_hits += 1;
        }
        if pc == PC_NULL_PATH {
            null_hits += 1;
        }
        if pc == PC_LOFLIST_WRITE {
            let r = &amiga.cpu().regs;
            let a0 = r.a[0];
            let a1 = r.a[1];
            // Actually a[0]=A0, a[1]=A1, a[2]=A2, a[3]=A3
            let a3 = r.a[3];
            let value = amiga.read_long(a0.wrapping_add(4));
            let frame = tick / PAL_FRAME_TICKS;
            writes.push((frame, a1, a0, a3, value));
        }
        prev_pc = pc;
    }
    eprintln!("Non-null-branch hits ($FCD5B8): {nonnull_hits}");
    eprintln!("NULL-branch hits ($FCD5D8):     {null_hits}");
    eprintln!("LOFlist-write hits ($FCD5C2):   {}", writes.len());
    for (frame, a1, a0, a3, val) in writes.iter().take(10) {
        eprintln!(
            "  frame~{frame:<3}  View=a1=${a1:08X}  LOFCprList=a0=${a0:08X}  GfxBase=a3=${a3:08X}  value=${val:08X}"
        );
    }
}

#[test]
#[ignore]
fn slow_ram_branches() {
    trap("slow-RAM", true);
}

#[test]
#[ignore]
fn chip_only_branches() {
    trap("chip-only", false);
}
