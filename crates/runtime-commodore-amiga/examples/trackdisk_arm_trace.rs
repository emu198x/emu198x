//! Trace trackdisk's execution between its final Wait($300) (tick
//! 13001190) and its switch to Wait($400) (tick 13006623). Capture:
//!   - every PC in the trackdisk region
//!   - writes to INTENA, INTREQ, DMACON, DSKLEN, DSKPT
//!   - every Signal() with source context

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

    let execbase: u32 = 0x00C00276;
    let wait_lvo = execbase.wrapping_sub(0x13E);

    // Window of interest: just before the switch to Wait($400).
    // Start capturing after tick 13_000_000 until 13_010_000.
    let capture_start: u64 = 13_000_000;
    let capture_end: u64 = 13_020_000;

    let mut pc_seq: Vec<(u64, u32)> = Vec::new();
    let mut prev_pc = 0u32;
    let mut wait_events: Vec<(u64, u32, u32)> = Vec::new(); // tick, thistask, mask
    let mut prev_wait_hit = false;

    let mut intena_writes: Vec<(u64, u16)> = Vec::new();
    let mut intreq_writes: Vec<(u64, u16)> = Vec::new();
    let mut dmacon_writes: Vec<(u64, u16)> = Vec::new();
    let mut dsklen_writes: Vec<(u64, u16)> = Vec::new();
    let mut prev_intena = amiga.paula.intena;
    let mut prev_dmacon = 0u16;
    let mut prev_dsklen_snapshot = amiga.paula.dsklen;

    let rl = |amiga: &Amiga, a: u32| -> u32 {
        (u32::from(amiga.memory.read_word(a)) << 16)
            | u32::from(amiga.memory.read_word(a.wrapping_add(2)))
    };

    for tick in 0..(500 * ccks_per_frame) {
        amiga.tick_cck();
        if tick < capture_start || tick > capture_end { continue; }

        let pc = amiga.cpu.instr_start_pc;
        if pc != prev_pc {
            if (0x00FE9000..0x00FEA800).contains(&pc) {
                pc_seq.push((tick, pc));
            }
            prev_pc = pc;
        }

        if pc == wait_lvo {
            if !prev_wait_hit {
                let tt = rl(&amiga, execbase.wrapping_add(0x114));
                let mask = amiga.cpu.regs.d[0];
                wait_events.push((tick, tt, mask));
            }
            prev_wait_hit = true;
        } else {
            prev_wait_hit = false;
        }

        let ie = amiga.paula.intena;
        if ie != prev_intena {
            intena_writes.push((tick, ie));
            prev_intena = ie;
        }
        let dmac = amiga.agnus.dmacon;
        if dmac != prev_dmacon {
            dmacon_writes.push((tick, dmac));
            prev_dmacon = dmac;
        }
        let dl = amiga.paula.dsklen;
        if dl != prev_dsklen_snapshot {
            dsklen_writes.push((tick, dl));
            prev_dsklen_snapshot = dl;
        }
        // Could also check dskpt.
        let _ = intreq_writes;
    }

    println!("── Wait() events in window ──");
    for (tick, tt, mask) in &wait_events {
        println!("  tick={tick} thistask=${tt:08X} Wait(${mask:08X})");
    }

    println!("\n── Distinct trackdisk PCs in window ({} entries) ──", pc_seq.len());
    for (tick, pc) in pc_seq.iter() {
        println!("  tick={tick} PC=${pc:08X}");
    }

    println!("\n── INTENA writes ──");
    for (tick, v) in &intena_writes {
        let bits: Vec<&str> = [
            (14,"MASTER"),(13,"EXTER"),(12,"DSKSYNC"),(11,"RBF"),
            (10,"AUD3"),(9,"AUD2"),(8,"AUD1"),(7,"AUD0"),
            (6,"BLIT"),(5,"VERTB"),(4,"COPER"),(3,"PORTS"),
            (2,"SOFTINT"),(1,"DSKBLK"),(0,"TBE"),
        ].iter().filter_map(|(bit,name)| if v & (1<<bit) != 0 { Some(*name) } else { None }).collect();
        println!("  tick={tick} INTENA=${v:04X} [{}]", bits.join(","));
    }

    println!("\n── DMACON writes ──");
    for (tick, v) in &dmacon_writes {
        println!("  tick={tick} DMACON=${v:04X}");
    }

    println!("\n── DSKLEN writes ──");
    for (tick, v) in &dsklen_writes {
        println!("  tick={tick} DSKLEN=${v:04X}");
    }
}
