//! After strap issues DoIO(CMD_READ) for the bootblock, sample PCs to
//! find where trackdisk is hanging.

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

    let mut pc_hist: HashMap<u32, u64> = HashMap::new();
    let mut armed = false;
    let mut trigger_tick: u64 = 0;

    let total_ticks = 500u64 * ccks_per_frame;
    for tick in 0..total_ticks {
        amiga.tick_cck();
        if !armed && amiga.cpu.instr_start_pc == 0x00FE859C {
            armed = true;
            trigger_tick = tick;
        }
        if !armed {
            continue;
        }
        // Sample every tick but only record PC changes in trackdisk range.
        let pc = amiga.cpu.instr_start_pc;
        if (0x00FE8000..0x00FEA000).contains(&pc) {
            *pc_hist.entry(pc).or_insert(0) += 1;
        }
    }

    let mut entries: Vec<(u32, u64)> = pc_hist.into_iter().collect();
    entries.sort_by(|a, b| b.1.cmp(&a.1));
    println!("Trigger (JSR DoIO CMD_READ) at tick {trigger_tick}");
    println!("Top 50 trackdisk/strap PCs (all hits) after trigger:");
    entries.sort_by_key(|&(pc, _)| pc);
    for (pc, count) in entries.iter().take(80) {
        println!("  ${pc:08X}: {count:>8} hits");
    }

    println!("\nFloppy final: motor_on={} spinning={} selected={} cyl={} head={}",
        amiga.floppy.motor_on(), amiga.floppy.motor_spinning(),
        amiga.floppy.selected(), amiga.floppy.cylinder(), amiga.floppy.head());
    println!("DSKLEN=${:04X} DSKPT=${:08X} DSKSYNC=${:04X} ADKCON=${:04X}",
        amiga.paula.dsklen, amiga.agnus.dsk_pt, amiga.paula.dsksync, amiga.paula.adkcon);
    println!("INTENA=${:04X} INTREQ=${:04X}",
        amiga.paula.intena, amiga.paula.intreq);
    println!("CIA-A: TA=${:04X} running={} TB=${:04X} running={} ICR_mask=${:02X} ICR_status=${:02X}",
        amiga.cia_a.timer_a(), amiga.cia_a.timer_a_running(),
        amiga.cia_a.timer_b(), amiga.cia_a.timer_b_running(),
        amiga.cia_a.icr_mask(), amiga.cia_a.icr_status());
    println!("CIA-B: TA=${:04X} running={} TB=${:04X} running={} ICR_mask=${:02X} ICR_status=${:02X}",
        amiga.cia_b.timer_a(), amiga.cia_b.timer_a_running(),
        amiga.cia_b.timer_b(), amiga.cia_b.timer_b_running(),
        amiga.cia_b.icr_mask(), amiga.cia_b.icr_status());
    println!("CIA-A TOD counter=${:06X} halted={} alarm=${:06X}",
        amiga.cia_a.tod_counter(), amiga.cia_a.tod_halted(), amiga.cia_a.tod_alarm());
    println!("CIA-B TOD counter=${:06X} halted={} alarm=${:06X}",
        amiga.cia_b.tod_counter(), amiga.cia_b.tod_halted(), amiga.cia_b.tod_alarm());
}
