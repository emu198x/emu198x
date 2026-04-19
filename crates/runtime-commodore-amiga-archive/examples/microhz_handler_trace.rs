//! Watch PCs around the Timer B PORTS underflow at tick 13334154 to
//! see whether ciaa.resource dispatches to timer.device's MICROHZ
//! handler at $FE93A6.

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

    const CIAARES_PORTS_CODE: u32 = 0x00FC4684;
    const TIMER_MICROHZ_CODE: u32 = 0x00FE93A6;
    const TRACKDISK_TASK: u32 = 0x00C0485E;

    let execbase: u32 = 0x00C00276;
    let signal_lvo = execbase.wrapping_sub(0x144);

    let mut ciaa_hits = 0u64;
    let mut microhz_hits = 0u64;
    let mut trackdisk_signals: Vec<(u64, u32)> = Vec::new();
    let mut pc_seq_around_first_fire: Vec<(u64, u32)> = Vec::new();
    let mut pc_seq_around_second_fire: Vec<(u64, u32)> = Vec::new();
    let mut prev_pc = 0u32;
    let mut prev_ciaa = false;
    let mut prev_microhz = false;

    for tick in 0..(500 * ccks_per_frame) {
        amiga.tick_cck();
        let pc = amiga.cpu.instr_start_pc;

        if pc == CIAARES_PORTS_CODE && !prev_ciaa {
            ciaa_hits += 1;
        }
        prev_ciaa = pc == CIAARES_PORTS_CODE;

        if pc == TIMER_MICROHZ_CODE && !prev_microhz {
            microhz_hits += 1;
        }
        prev_microhz = pc == TIMER_MICROHZ_CODE;

        if pc == signal_lvo {
            let tgt = amiga.cpu.regs.a[1];
            let mask = amiga.cpu.regs.d[0];
            if tgt == TRACKDISK_TASK {
                trackdisk_signals.push((tick, mask));
            }
        }

        // Capture PCs in tight window around first expected PORTS fire
        // (~tick 13334154).
        if tick >= 13_334_000 && tick <= 13_335_000 && pc != prev_pc {
            pc_seq_around_first_fire.push((tick, pc));
            prev_pc = pc;
        }
        if tick >= 13_662_000 && tick <= 13_664_500 && pc != prev_pc {
            pc_seq_around_second_fire.push((tick, pc));
            prev_pc = pc;
        }
    }

    println!("ciaa.resource PORTS handler ($FC4684) entries: {ciaa_hits}");
    println!("timer.device MICROHZ handler ($FE93A6) entries: {microhz_hits}");

    println!("\nSignal(trackdisk) calls:");
    for (tick, mask) in &trackdisk_signals {
        println!("  tick={tick} mask=${mask:08X}");
    }

    println!("\nPCs around SECOND PORTS fire (tick 13662000..13664500, {} entries):", pc_seq_around_second_fire.len());
    for (tick, pc) in pc_seq_around_second_fire.iter().take(80) {
        println!("  tick={tick} PC=${pc:08X}");
    }
}
