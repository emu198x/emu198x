//! Detect whether trackdisk.device's task code ever executes after strap
//! issues DoIO(TD_CHANGESTATE). If not, the task is never being scheduled,
//! which points to a broken signal/wait path between strap and trackdisk.

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

    // Strap hits $FE8570 (DoIO TD_CHANGESTATE) at tick ~13,002,035.
    // Anything after that in trackdisk range ($FE9000-$FEA000) is the task.
    // Also monitor whether any "custom write" to Paula disk regs happens.

    // Count fetches in various ranges after strap hits DoIO.
    let mut fc_fetches = 0u64;
    let mut trackdisk_fetches = 0u64;
    let mut other_fetches = 0u64;
    let mut reached_trigger = false;
    let mut prev_pc = u32::MAX;

    let total_ticks = 500u64 * ccks_per_frame;
    for _tick in 0..total_ticks {
        amiga.tick_cck();
        let pc = amiga.cpu.instr_start_pc;
        if pc == 0x00FE8570 {
            reached_trigger = true;
        }
        if !reached_trigger || pc == prev_pc {
            continue;
        }
        prev_pc = pc;
        if pc >= 0x00FE9000 && pc < 0x00FEB000 {
            trackdisk_fetches += 1;
        } else if pc >= 0x00FC0000 && pc < 0x00FE9000 {
            fc_fetches += 1;
        } else {
            other_fetches += 1;
        }
    }

    println!("After strap's DoIO(TD_CHANGESTATE) trigger (tick ~13M):");
    println!("  Unique PCs in $FE9000-$FEB000 (trackdisk): {trackdisk_fetches}");
    println!("  Unique PCs in $FC0000-$FE9000 (exec/gfx/intuition): {fc_fetches}");
    println!("  Unique PCs elsewhere (chip RAM tasks): {other_fetches}");
    println!(
        "Final: PC=${:08X} DSKLEN=${:04X} DSKPT=${:08X}",
        amiga.cpu.instr_start_pc,
        amiga.paula.dsklen,
        amiga.agnus.dsk_pt
    );
}
