//! Task #96 experiment: if we disable the copper entirely, does
//! chip-only boot progress past romboot's TD_CHANGESTATE?
//!
//! We do it the quick-and-dirty way by patching DMACON after each
//! tick to clear COPEN. This is NOT a fix — it's a diagnostic that
//! proves the copper is the source of chip-only's deadlock.

use std::path::PathBuf;
use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() { return None; }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

#[test]
#[ignore]
fn chip_only_with_copper_neutered() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::new(rom);

    // Hit counts on the romboot milestones we care about.
    let targets: &[(u32, &str)] = &[
        (0x00FE_8444, "romboot entry"),
        (0x00FE_8560, "post CMD_CLEAR"),
        (0x00FE_8574, "post TD_CHANGESTATE (chip-only never reaches)"),
        (0x00FE_85A0, "post CMD_READ"),
        (0x00FE_8610, "insert-disk-setup caller"),
        (0x00FE_8732, "insert-disk-setup entry"),
        (0x00FE_8888, "LoadView JSR inside setup"),
    ];
    let mut counts = vec![0u64; targets.len()];
    let mut prev_pc = amiga.cpu().regs.pc;

    for _ in 0..(400u64 * PAL_FRAME_TICKS as u64) {
        amiga.tick();
        // Diagnostic: force COPEN off every tick. NOT A FIX —
        // just proves the copper is the corruption source.
        amiga.poke_word(0x00DF_F096, 0x0080);
        let pc = amiga.cpu().regs.pc;
        if pc == prev_pc { continue; }
        for (i, (tpc, _)) in targets.iter().enumerate() {
            if pc == *tpc {
                counts[i] += 1;
            }
        }
        prev_pc = pc;
    }
    eprintln!("=== chip-only with copper neutered ===");
    for ((pc, desc), c) in targets.iter().zip(counts.iter()) {
        eprintln!("  ${pc:08X}  {c:>4}  {desc}");
    }
    eprintln!("\nFinal PC = ${:08X}", amiga.cpu().regs.pc);
    eprintln!("Final INTENA = ${:04X}", amiga.intena() & 0x7FFF);
}
