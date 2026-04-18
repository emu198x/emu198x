//! Log all CIA-A ICR reads around the first and second Timer B fires.

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

    let mut icr_events: Vec<(u64, u8, u8)> = Vec::new(); // tick, status_before_read, status_after
    let mut prev_status = 0u8;
    let mut just_changed = None;

    // We watch icr_status value each tick. A fall to 0 indicates a read
    // happened.
    for tick in 0..(500 * ccks_per_frame) {
        amiga.tick_cck();
        let st = amiga.cia_a.icr_status();
        if st != prev_status {
            if tick >= 13_330_000 && tick <= 13_670_000 {
                icr_events.push((tick, prev_status, st));
            }
            prev_status = st;
            just_changed = Some(tick);
        }
        let _ = just_changed;
    }

    println!("── CIA-A icr_status transitions (13_330_000..13_670_000) ──");
    for (tick, before, after) in &icr_events {
        println!("  tick={tick} {before:02X} -> {after:02X}");
    }
}
