//! Trace CIA-A timer B arming and interrupt behavior around trackdisk's
//! motor-settle timer request at tick 13006179.

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

    let mut cia_a_icr_changes: Vec<(u64, u8, u8)> = Vec::new();
    let mut initial_mask: u8 = 0;
    let mut mask_logged = false;
    let mut cia_b_icr_changes: Vec<(u64, u8, u8)> = Vec::new();
    let mut cia_a_crb_changes: Vec<(u64, u8)> = Vec::new();
    let mut cia_a_irq_rises = 0u64;
    let mut cia_b_irq_rises = 0u64;
    let mut cia_a_tb_sample: Vec<(u64, u16)> = Vec::new();
    let mut ports_fires: Vec<(u64, u16)> = Vec::new();
    let mut exter_fires: Vec<(u64, u16)> = Vec::new();
    let mut prev_a_mask = amiga.cia_a.icr_mask();
    let mut prev_a_status = amiga.cia_a.icr_status();
    let mut prev_b_mask = amiga.cia_b.icr_mask();
    let mut prev_b_status = amiga.cia_b.icr_status();
    let mut prev_a_crb = 0u8;
    let mut prev_a_irq = false;
    let mut prev_b_irq = false;
    let mut prev_ports_set = false;
    let mut prev_exter_set = false;
    let mut sample_due = 0u64;

    for tick in 0..(500 * ccks_per_frame) {
        amiga.tick_cck();

        if !mask_logged && tick >= 13_000_000 {
            initial_mask = amiga.cia_a.icr_mask();
            mask_logged = true;
        }

        let am = amiga.cia_a.icr_mask();
        let as_ = amiga.cia_a.icr_status();
        if am != prev_a_mask || as_ != prev_a_status {
            if tick >= 13_000_000 && tick <= 14_000_000 {
                cia_a_icr_changes.push((tick, am, as_));
            }
            prev_a_mask = am; prev_a_status = as_;
        }
        let bm = amiga.cia_b.icr_mask();
        let bs = amiga.cia_b.icr_status();
        if bm != prev_b_mask || bs != prev_b_status {
            if tick >= 13_000_000 && tick <= 13_020_000 {
                cia_b_icr_changes.push((tick, bm, bs));
            }
            prev_b_mask = bm; prev_b_status = bs;
        }
        // Sample timer B counter every 100 ticks around the window.
        if tick >= 13_006_000 && tick <= 14_000_000 && tick >= sample_due {
            let tb = amiga.cia_a.timer_b();
            cia_a_tb_sample.push((tick, tb));
            sample_due = tick + 10_000;
        }

        let ai = amiga.cia_a.irq_active();
        if ai && !prev_a_irq { cia_a_irq_rises += 1; }
        prev_a_irq = ai;
        let bi = amiga.cia_b.irq_active();
        if bi && !prev_b_irq { cia_b_irq_rises += 1; }
        prev_b_irq = bi;

        let ports_now = (amiga.paula.intreq & 0x0008) != 0;
        if ports_now && !prev_ports_set {
            if tick >= 13_000_000 && tick <= 14_000_000 {
                ports_fires.push((tick, amiga.paula.intreq));
            }
        }
        prev_ports_set = ports_now;
        let exter_now = (amiga.paula.intreq & 0x2000) != 0;
        if exter_now && !prev_exter_set {
            if tick >= 13_000_000 && tick <= 13_030_000 {
                exter_fires.push((tick, amiga.paula.intreq));
            }
        }
        prev_exter_set = exter_now;
    }

    println!("CIA-A initial icr_mask at tick 13_000_000: ${initial_mask:02X}");
    println!("── CIA-A ICR activity in window 13_000_000..14_000_000 ──");
    for (tick, m, s) in &cia_a_icr_changes {
        println!("  tick={tick} CIA-A icr_mask=${m:02X} icr_status=${s:02X}");
    }

    println!("\n── CIA-B ICR activity in window ──");
    for (tick, m, s) in &cia_b_icr_changes {
        println!("  tick={tick} CIA-B icr_mask=${m:02X} icr_status=${s:02X}");
    }

    println!("\n── CIA-A timer B sampled (first 40) ──");
    for (tick, tb) in cia_a_tb_sample.iter().take(40) {
        println!("  tick={tick} TB=${tb:04X}");
    }

    println!("\nCIA-A irq_active rises (500 frames): {cia_a_irq_rises}");
    println!("CIA-B irq_active rises (500 frames): {cia_b_irq_rises}");
    println!("PORTS INTREQ fires in window: {}", ports_fires.len());
    for (t, v) in &ports_fires { println!("  tick={t} INTREQ=${v:04X}"); }
    println!("EXTER INTREQ fires in window: {}", exter_fires.len());
    for (t, v) in &exter_fires { println!("  tick={t} INTREQ=${v:04X}"); }
}
