//! Task #96 — exact log of every CPU write to `GfxBase->LOFlist`.
//!
//! Watches `GfxBase+$32..GfxBase+$36` (4 bytes) and logs tick, PC,
//! exact address (byte/word offset), value, and word vs byte.

use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};
use std::path::PathBuf;

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

fn dump(label: &str, use_slow_ram: bool) {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = if use_slow_ram {
        AmigaOcs::with_slow_ram(rom, 512 * 1024)
    } else {
        AmigaOcs::new(rom)
    };
    let gfx_base = if use_slow_ram {
        0x00C0_1E1E
    } else {
        0x0000_221E
    };
    // Watch GfxBase+$32 .. GfxBase+$36 (the LOFlist longword).
    amiga.debug_watch_addr = Some((gfx_base + 0x32, 4));

    for _ in 0..(200u64 * PAL_FRAME_TICKS) {
        amiga.tick();
    }

    eprintln!("\n########## {label} ##########");
    eprintln!(
        "GfxBase+$32 (LOFlist) write log: {} entries",
        amiga.debug_watch_writes.len()
    );
    for (cck, pc, addr, val, is_word) in amiga.debug_watch_writes.iter() {
        let frame = cck / 70824;
        let size = if *is_word { 'W' } else { 'B' };
        eprintln!("  frame~{frame:<3}  pc=${pc:08X}  {size} write to ${addr:08X} = ${val:04X}");
    }
}

#[test]
#[ignore]
fn slow_ram_loflist_writes() {
    dump("slow-RAM", true);
}

#[test]
#[ignore]
fn chip_only_loflist_writes() {
    dump("chip-only", false);
}
