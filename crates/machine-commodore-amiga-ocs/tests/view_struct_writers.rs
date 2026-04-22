//! Task #96 — who populates the View struct at slow-RAM $5A10?
//!
//! The "second LoadView" at slow-RAM frame 188 uses a View that
//! lives at chip-RAM $5A10 with fields:
//!   +0  ViewPort   = $000059E8
//!   +4  LOFCprList = $00C01808
//!   +8  SHFCprList = 0
//!   +12 DyOffset   = $002C
//!   +14 DxOffset   = $0081
//!   +16 Modes      = 0
//!
//! In chip-only that memory is uninitialised, so whoever writes
//! those fields isn't running. Watch the 24-byte View range
//! $5A10..$5A28 in slow-RAM, log every write with PC.

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

#[test]
#[ignore]
fn slow_ram_view_struct_writers() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);
    // Watch the full View struct range (24 bytes = three longs + two
    // words + one long) plus a bit more for safety — $5A10..$5A30.
    amiga.debug_watch_addr = Some((0x0000_5A10, 0x20));
    for _ in 0..(200u64 * PAL_FRAME_TICKS as u64) {
        amiga.tick();
    }
    eprintln!(
        "=== View @ $5A10 write log ({} entries) ===",
        amiga.debug_watch_writes.len()
    );
    for (cck, pc, addr, val, is_word) in amiga.debug_watch_writes.iter() {
        let frame = cck / 70824;
        let size = if *is_word { 'W' } else { 'B' };
        eprintln!("  frame~{frame:<3}  pc=${pc:08X}  {size} ${addr:08X} = ${val:04X}");
    }
}

#[test]
#[ignore]
fn chip_only_view_struct_region_writers() {
    // For chip-only, watch the same range. Should see very little —
    // whoever wrote the slow-RAM View isn't running.
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::new(rom);
    amiga.debug_watch_addr = Some((0x0000_5A10, 0x20));
    for _ in 0..(200u64 * PAL_FRAME_TICKS as u64) {
        amiga.tick();
    }
    eprintln!(
        "=== chip-only $5A10..$5A30 write log ({} entries) ===",
        amiga.debug_watch_writes.len()
    );
    for (cck, pc, addr, val, is_word) in amiga.debug_watch_writes.iter() {
        let frame = cck / 70824;
        let size = if *is_word { 'W' } else { 'B' };
        eprintln!("  frame~{frame:<3}  pc=${pc:08X}  {size} ${addr:08X} = ${val:04X}");
    }
}
