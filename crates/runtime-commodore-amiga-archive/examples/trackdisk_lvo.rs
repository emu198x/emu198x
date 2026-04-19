//! Read the trackdisk LVO table to find BeginIO's ROM target,
//! so we know where trackdisk's IO dispatch entry actually is.

use emu198x_shell::{MediaKind, read_media_asset};
use machine_commodore_amiga::Amiga;
use std::fs;
use std::path::Path;

fn main() {
    let kickstart = fs::read("/Users/stevehill/.emu198x/roms/commodore-amiga/kick13.rom").unwrap();
    let mut amiga = Amiga::new_with_slow_ram(kickstart, 512 * 1024);

    let disk_path = "/Users/stevehill/Projects/Emu198x-Unclean/Reference/amiga/Operating Systems/Workbench/Workbench v1.3.3 rev 34.34 (1990)(Commodore)(Disk 1 of 2)(Workbench)[Cloanto Amiga Forever Edition].zip";
    let loaded = read_media_asset(Path::new(disk_path), MediaKind::Disk).unwrap();
    let adf = format_commodore_amiga_adf::Adf::from_bytes(loaded.bytes).unwrap();
    amiga.insert_disk(adf);
    amiga.floppy.acknowledge_disk_change();

    let ccks_per_frame = u64::from(amiga.agnus.lines_per_frame)
        * u64::from(commodore_agnus_ocs::PAL_CCKS_PER_LINE);

    // Give boot enough time to set up trackdisk.
    for _ in 0..200u64 * ccks_per_frame {
        amiga.tick_cck();
    }

    let trackdisk_base: u32 = 0x00C03AA4;
    let rl = |amiga: &Amiga, a: u32| -> u32 {
        (u32::from(amiga.memory.read_word(a)) << 16)
            | u32::from(amiga.memory.read_word(a.wrapping_add(2)))
    };
    let rw = |amiga: &Amiga, a: u32| amiga.memory.read_word(a);

    println!("trackdisk.device LibNode at ${trackdisk_base:08X}:");
    for off in [-6i32, -12, -18, -24, -30, -36, -42, -48, -54, -60] {
        let lvo_addr = trackdisk_base.wrapping_add(off as u32);
        let jmp = rw(&amiga, lvo_addr);
        let target = rl(&amiga, lvo_addr.wrapping_add(2));
        let label = match off {
            -6 => "Open",
            -12 => "Close",
            -18 => "Expunge",
            -24 => "Reserved",
            -30 => "BeginIO",
            -36 => "AbortIO",
            _ => "?",
        };
        println!("  lvo {off:>4} ({label}): JMP_op=${jmp:04X} target=${target:08X}");
    }
}
