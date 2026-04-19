use std::path::PathBuf;
use machine_commodore_amiga::Amiga;

#[test]
#[ignore]
fn chip_only_pc_progression() {
    let home = std::env::var("HOME").unwrap();
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    let kickstart = std::fs::read(&path).unwrap();
    let mut amiga = Amiga::new(kickstart);

    let mut last_pc: u32 = 0;
    let mut stuck_frames = 0u32;
    for frame in 0..500 {
        amiga.run_frame();
        let pc = amiga.cpu.regs.pc;
        if pc == last_pc {
            stuck_frames += 1;
        } else {
            if stuck_frames > 0 {
                eprintln!("frame {}: PC=${:08X} (was stuck at ${:08X} for {} frames)",
                    frame, pc, last_pc, stuck_frames);
            } else {
                eprintln!("frame {}: PC=${:08X}", frame, pc);
            }
            stuck_frames = 0;
            last_pc = pc;
        }
        // Sample every 50 frames regardless
        if frame % 50 == 0 {
            eprintln!("  [@ frame {}] PC=${:08X} SR=${:04X} DMACON=${:04X} BPLCON0=${:04X} COP1LC=${:08X}",
                frame, pc, amiga.cpu.regs.sr,
                amiga.agnus.dmacon, amiga.agnus.bplcon0,
                amiga.copper.cop1lc);
        }
    }
}
