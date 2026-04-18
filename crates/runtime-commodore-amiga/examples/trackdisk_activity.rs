//! After the CIA double-read fix, count total DoIO calls to trackdisk
//! over a long boot to see if AmigaDOS is actually exercising it.

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

    const TDSK_BEGINIO: u32 = 0x00FE9C3E;

    let mut trackdisk_beginio_hits = 0u64;
    let mut dsklen_arms: Vec<(u64, u16)> = Vec::new();
    let mut cyl_changes: Vec<(u64, u32)> = Vec::new();
    let mut dskblk_int_rises = 0u64;
    let mut prev_pc = 0u32;
    let mut prev_dsklen_armed = false;
    let mut prev_cyl = 0u32;
    let mut prev_dskblk_set = false;

    let total_frames = 3000u64;
    for tick in 0..(total_frames * ccks_per_frame) {
        amiga.tick_cck();
        let pc = amiga.cpu.instr_start_pc;
        if pc == TDSK_BEGINIO && pc != prev_pc {
            trackdisk_beginio_hits += 1;
        }
        prev_pc = pc;

        let cyl = amiga.floppy.cylinder();
        if cyl != prev_cyl {
            if cyl_changes.len() < 200 {
                cyl_changes.push((tick, cyl));
            }
            prev_cyl = cyl;
        }

        let armed = amiga.paula.dsklen & 0x8000 != 0;
        if armed && !prev_dsklen_armed && dsklen_arms.len() < 50 {
            dsklen_arms.push((tick, amiga.paula.dsklen));
        }
        prev_dsklen_armed = armed;

        let dskblk = (amiga.paula.intreq & 0x0002) != 0;
        if dskblk && !prev_dskblk_set { dskblk_int_rises += 1; }
        prev_dskblk_set = dskblk;
    }

    println!("── trackdisk + disk DMA activity over {total_frames} frames ──");
    println!("trackdisk BeginIO entries: {trackdisk_beginio_hits}");
    println!("DSKLEN arms (DMA starts):  {}", dsklen_arms.len());
    for (tick, dl) in dsklen_arms.iter().take(30) {
        println!("    tick={tick} DSKLEN=${dl:04X}");
    }
    println!("DSKBLK INTREQ rises:       {dskblk_int_rises}");
    println!("Cylinder transitions ({}):", cyl_changes.len());
    for (tick, cy) in cyl_changes.iter() {
        println!("    tick={tick} cyl={cy}");
    }

    println!("\nFinal: PC=${:08X} motor={} spin={} sel={} cyl={}",
        amiga.cpu.instr_start_pc, amiga.floppy.motor_on(),
        amiga.floppy.motor_spinning(), amiga.floppy.selected(),
        amiga.floppy.cylinder());
}
