//! Capture CIA-A ICR reads that happen via actual bus cycles.

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

    let mut seen: Vec<(u64, String)> = Vec::new();

    for tick in 0..(500 * ccks_per_frame) {
        let len_before = amiga.debug_cia_a_read_log.len();
        amiga.tick_cck();
        let len_after = amiga.debug_cia_a_read_log.len();
        if len_after > len_before {
            // One or more log entries added this tick.
            for i in len_before..len_after {
                if let Some(s) = amiga.debug_cia_a_read_log.get(i) {
                    // Only record ICR reads (reg=$0D).
                    if s.contains("reg=$0D") {
                        seen.push((tick, s.clone()));
                    }
                }
            }
        }
        // The log has a max size of 64 — after overflow it pops front.
        // For simplicity, focus on a narrow window around the PORTS fires.
    }

    println!("CIA-A ICR reads captured in log:");
    for (tick, s) in seen.iter().filter(|(t, _)| *t >= 13_330_000 && *t <= 13_680_000) {
        println!("  tick={tick} {s}");
    }
}
