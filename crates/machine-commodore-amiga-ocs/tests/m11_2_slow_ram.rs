//! M11.2: slow RAM (trapdoor expansion at $C00000).
//!
//! Per `wiki/decisions/amiga-restart-plan.md`. Adds 512K trapdoor
//! slow RAM to see if the boot progresses past the chip-only
//! deadlock. The archived chip-only investigation observed that
//! slow-RAM KS 1.3 did reach display setup (even though the chip-only
//! path didn't).

use std::path::PathBuf;
use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_LINES, PAL_LINE_CCKS};

// frame_ccks below is in Agnus beam-coordinate units (CCKs); cck_count()
// also returns CCKs, so the while-loop gating is unchanged by the
// master/4 tick migration.

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
fn slow_ram_accessible_at_c00000() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);

    // Write/read a byte through the slow-RAM window.
    amiga.poke_byte(0x00C00100, 0x42);
    assert_eq!(amiga.read_word(0x00C00100) & 0xFF00, 0x4200);
}

#[test]
fn slow_ram_not_present_by_default() {
    let Some(rom) = load_kickstart() else { return };
    let amiga = AmigaOcs::new(rom);
    // Reads from slow-RAM range return floating bus when not present.
    assert_eq!(amiga.read_word(0x00C00000), 0xFFFF);
}

#[test]
#[ignore]
fn diagnostic_boot_with_slow_ram() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);

    // Run for many frames to see how far slow-RAM boot progresses.
    let frame_ccks = u64::from(PAL_LINE_CCKS) * u64::from(PAL_FRAME_LINES);
    for checkpoint_frame in [50u64, 100, 200, 300, 500, 1000, 2000] {
        let target_cck = checkpoint_frame * frame_ccks;
        while amiga.cck_count() < target_cck {
            amiga.tick();
        }
        let pc = amiga.cpu().regs.pc;
        let sr = amiga.cpu().regs.sr;
        let exec_base = amiga.read_long(0x000004);
        let dmacon = amiga.dmacon();
        let intena = amiga.intena();
        let bplcon0 = amiga.bplcon0();
        let color00 = amiga.color(0);
        eprintln!(
            "  frame={checkpoint_frame:3} cck={cck:10} pc=${pc:08X} sr=${sr:04X} \
             exec_base=${exec_base:08X} dmacon=${dmacon:04X} intena=${intena:04X} \
             bplcon0=${bplcon0:04X} color00=${color00:04X}",
            cck = amiga.cck_count(),
        );
    }
}
