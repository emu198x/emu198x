//! Boot diagnostic — confirms what register state the OS programs
//! after a long settle. Gated behind a local ROM bundle.

use std::path::PathBuf;

use machine_atari_800xl::{Atari800xl, Atari800xlRegion};

fn rom_dir() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".emu198x/roms/atari-800xl"))
}

#[test]
#[ignore = "requires local OS + BASIC ROMs at ~/.emu198x/roms/atari-800xl/"]
fn basic_boot_programs_antic_and_gtia() {
    let dir = rom_dir().expect("HOME unset");
    let os = std::fs::read(dir.join("atarixl.rom")).expect("atarixl.rom");
    let basic = std::fs::read(dir.join("ataribas.rom")).expect("ataribas.rom");
    let mut sys = Atari800xl::new(Some(os), Some(basic), None, Atari800xlRegion::Ntsc, true)
        .expect("boot");
    eprintln!("frame  PC   DMACTL NMIEN DLIST COLBK COLPF2");
    for i in 1..=300 {
        sys.run_frame();
        if i == 1 || i == 5 || i == 10 || i == 30 || i == 60 || i == 120 || i == 300 {
            let pc = sys.cpu().regs.pc;
            let a = sys.antic();
            let g = sys.gtia();
            eprintln!(
                "{:5} ${:04X}  ${:02X}    ${:02X}   ${:04X} ${:02X}    ${:02X}",
                i,
                pc,
                a.dmactl_value(),
                a.nmien_value(),
                a.dlist_value(),
                g.colbk_value(),
                g.colpf_values()[2],
            );
        }
    }
    eprintln!("framebuffer first 8 px: {:?}", &sys.framebuffer()[..8]);
    let unique: std::collections::HashSet<u32> = sys.framebuffer().iter().copied().collect();
    eprintln!("unique framebuffer colours: {}", unique.len());
}
