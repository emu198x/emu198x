//! Task #96 — dump the cprlist struct after LoadView to see exactly
//! what data the ROM set up.
//!
//! Chip-only: frame 91 wrote LOFlist = $676 (= ExecBase). That
//! value came from `*(LOFCprList + 4)` where LOFCprList was $4E3C.
//! So the long at $4E40 should be $676 after the write.
//!
//! Let's verify directly and also look at what `View->ViewPort`,
//! `LOFCprList->next`, `LOFCprList->start` are at frame 92.

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
fn dump_chip_only_cprlist_and_view() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::new(rom);
    for _ in 0..(95 * PAL_FRAME_TICKS) {
        amiga.tick();
    }

    // chip-only known addresses:
    let view = 0x0000_49A6u32;
    let lof_cpr_list = 0x0000_4E3Cu32; // from branch trap
    let gfx_base = 0x0000_221Eu32;

    eprintln!("=== chip-only @ frame 95 ===");
    eprintln!("View @ ${view:08X}:");
    eprintln!("  ViewPort     = ${:08X}", amiga.read_long(view));
    eprintln!("  LOFCprList   = ${:08X}", amiga.read_long(view + 4));
    eprintln!("  SHFCprList   = ${:08X}", amiga.read_long(view + 8));
    eprintln!("  DyOffset     = ${:04X}", amiga.read_word(view + 12));
    eprintln!("  DxOffset     = ${:04X}", amiga.read_word(view + 14));
    eprintln!("  Modes        = ${:04X}", amiga.read_word(view + 16));
    eprintln!("LOFCprList @ ${lof_cpr_list:08X}:");
    eprintln!("  next         = ${:08X}", amiga.read_long(lof_cpr_list));
    eprintln!(
        "  start        = ${:08X}",
        amiga.read_long(lof_cpr_list + 4)
    );
    eprintln!(
        "  MaxCount     = ${:08X}",
        amiga.read_long(lof_cpr_list + 8)
    );
    eprintln!(
        "GfxBase->LOFlist = ${:08X}",
        amiga.read_long(gfx_base + 0x32)
    );
}

#[test]
#[ignore]
fn dump_slow_ram_cprlist_and_view() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);
    for _ in 0..(115 * PAL_FRAME_TICKS) {
        amiga.tick();
    }

    let view = 0x00C0_3D46u32;
    let lof_cpr_list = 0x00C0_41DCu32;
    let gfx_base = 0x00C0_1E1Eu32;

    eprintln!("=== slow-RAM @ frame 115 ===");
    eprintln!("View @ ${view:08X}:");
    eprintln!("  ViewPort     = ${:08X}", amiga.read_long(view));
    eprintln!("  LOFCprList   = ${:08X}", amiga.read_long(view + 4));
    eprintln!("  SHFCprList   = ${:08X}", amiga.read_long(view + 8));
    eprintln!("LOFCprList @ ${lof_cpr_list:08X}:");
    eprintln!("  next         = ${:08X}", amiga.read_long(lof_cpr_list));
    eprintln!(
        "  start        = ${:08X}",
        amiga.read_long(lof_cpr_list + 4)
    );
    eprintln!(
        "GfxBase->LOFlist = ${:08X}",
        amiga.read_long(gfx_base + 0x32)
    );
}
