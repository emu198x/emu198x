//! Sample CPU PC periodically to see where the ROM spends time during
//! post-boot idle with a disk inserted.

use emu198x_shell::{MediaKind, read_media_asset};
use machine_commodore_amiga::Amiga;
use std::collections::HashMap;
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

    // Run 800 frames; histogram of PC in buckets of 16 bytes.
    let mut pc_hist: HashMap<u32, u64> = HashMap::new();
    let total_ticks = 800u64 * ccks_per_frame;
    // Sample every 64 ticks.
    for tick in 0..total_ticks {
        amiga.tick_cck();
        if tick & 0x3F == 0 {
            let pc_bucket = amiga.cpu.instr_start_pc & !0xF;
            *pc_hist.entry(pc_bucket).or_insert(0) += 1;
        }
    }

    let mut entries: Vec<(u32, u64)> = pc_hist.into_iter().collect();
    entries.sort_by(|a, b| b.1.cmp(&a.1));
    println!("Top 20 PC hotspots (16-byte buckets), 800 frame run:");
    for (pc, count) in entries.iter().take(20) {
        println!("  ${pc:08X}: {count:>8} samples");
    }

    let reads_started = amiga.agnus.dsk_pt;
    println!("\nFinal DSKPT: ${reads_started:08X}");
    println!("Floppy: motor_on={} spinning={} selected={} has_disk={}",
        amiga.floppy.motor_on(),
        amiga.floppy.motor_spinning(),
        amiga.floppy.selected(),
        amiga.floppy.has_disk()
    );
}
