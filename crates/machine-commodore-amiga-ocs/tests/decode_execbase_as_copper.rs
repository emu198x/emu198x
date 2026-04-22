//! Task #96 deep dive: dump the ExecBase bytes at chip-only's
//! COP2LC=$0676 and decode them as copper instructions. Look for
//! a SKIP / WAIT that SHOULD stop the copper.

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

fn reg_name(reg: u16) -> &'static str {
    match reg {
        0x000 => "BLTDDAT",
        0x040 => "BLTCON0",
        0x042 => "BLTCON1",
        0x050 => "BLTCPTH",
        0x052 => "BLTCPTL",
        0x054 => "BLTBPTH",
        0x080 => "COP1LCH",
        0x082 => "COP1LCL",
        0x084 => "COP2LCH",
        0x086 => "COP2LCL",
        0x088 => "COPJMP1",
        0x08A => "COPJMP2",
        0x096 => "DMACON",
        0x09A => "INTENA",
        0x09C => "INTREQ",
        0x09E => "ADKCON",
        0x100 => "BPLCON0",
        0x102 => "BPLCON1",
        0x104 => "BPLCON2",
        0x180 => "COLOR00",
        _ => "???",
    }
}

#[test]
#[ignore]
fn decode_chip_only_execbase_as_copper() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::new(rom);
    // Run long enough that ExecBase has been populated with the
    // library struct.
    for _ in 0..(100u64 * PAL_FRAME_TICKS as u64) {
        amiga.tick();
    }

    let mut pc = 0x0676u32;
    eprintln!("=== Decoding chip-only ExecBase at ${pc:08X} as copper ===");
    for _ in 0..40 {
        let w1 = amiga.read_word(pc);
        let w2 = amiga.read_word(pc.wrapping_add(2));
        if w1 & 1 == 0 {
            // MOVE
            let reg = w1 & 0x1FE;
            let name = reg_name(reg);
            let stop_hint = if reg < 0x80 {
                " ← DANGEROUS (<$80) — stops copper!"
            } else {
                ""
            };
            eprintln!(
                "  ${pc:08X}  ${w1:04X} ${w2:04X}  MOVE ${reg:03X} ({name}) = ${w2:04X}{stop_hint}"
            );
        } else if w2 & 1 == 0 {
            // WAIT
            let vpos = (w1 >> 8) & 0xFF;
            let hpos = w1 & 0xFE;
            eprintln!(
                "  ${pc:08X}  ${w1:04X} ${w2:04X}  WAIT vp=${vpos:02X} hp=${hpos:02X} (BFD={})",
                if w2 & 0x8000 != 0 { 1 } else { 0 }
            );
            if w1 == 0xFFFF && w2 == 0xFFFE {
                eprintln!("      ← end-of-list (FFFE) — copper halts here");
                break;
            }
        } else {
            // SKIP
            let vpos = (w1 >> 8) & 0xFF;
            let hpos = w1 & 0xFE;
            eprintln!("  ${pc:08X}  ${w1:04X} ${w2:04X}  SKIP vp=${vpos:02X} hp=${hpos:02X}");
        }
        pc = pc.wrapping_add(4);
    }
}
