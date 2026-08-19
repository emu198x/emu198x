//! Diagnostic — sample CPU + chipset state at various points to see
//! how far the boot gets with the current M5 emulator.
//!
//! Not a milestone test. Used to plan subsequent milestones by
//! observing what the boot is asking for next.

use machine_commodore_amiga_ocs::AmigaOcs;
use std::path::PathBuf;

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        emu198x_test_skip::record(&format!(
            "skipping: Kickstart 1.3 ROM missing at {}",
            path.display()
        ));
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

#[test]
#[ignore]
fn diagnostic_long_run() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::new(rom);

    let checkpoints = [500_000u64, 5_000_000, 50_000_000, 200_000_000, 500_000_000];
    let mut last_cck = 0u64;

    for cp in checkpoints {
        while amiga.cck_count() < cp {
            amiga.tick();
        }
        let pc = amiga.cpu().regs.pc;
        let sr = amiga.cpu().regs.sr;
        let ssp = amiga.cpu().regs.ssp;
        let exec_base = amiga.read_long(0x000004);
        let dmacon = amiga.dmacon();
        let intena = amiga.intena();
        let bplcon0 = amiga.bplcon0();
        let color00 = amiga.color(0);
        // Read AttnFlags+AttnResched as a longword from ExecBase+$126.
        let attn = if exec_base != 0 && exec_base < 0x100_0000 {
            amiga.read_long(exec_base.wrapping_add(0x126))
        } else {
            0
        };
        eprintln!(
            "  cck={cck:9} pc=${pc:08X} sr=${sr:04X} ssp=${ssp:08X} \
             exec_base=${exec_base:08X} attn=${attn:08X} \
             dmacon=${dmacon:04X} intena=${intena:04X} (peak ${peak:04X}, writes {writes}) \
             bplcon0=${bplcon0:04X} color00=${color00:04X} \
             ovl={ovl}",
            cck = amiga.cck_count(),
            ovl = amiga.memory().overlay(),
            peak = amiga.debug_peak_intena,
            writes = amiga.debug_intena_writes,
        );
        last_cck = amiga.cck_count();
    }
    let _ = last_cck;
}
