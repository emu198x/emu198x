//! Trace strap's DoIO(CMD_READ) flow.
//!
//! At tick when strap hits $FE859C (DoIO), dump the IORequest it's using,
//! then follow PC for a fixed window, logging hits on:
//!   - trackdisk BeginIO ($FE9C3E)
//!   - exec PutMsg vector ($FFFFFE2A = -$16E(ExecBase)... actually the
//!     implementation address)
//!   - exec Signal
//!   - any return to $FE85A0 (DoIO would-have-returned)

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

    let rl = |amiga: &Amiga, a: u32| -> u32 {
        (u32::from(amiga.memory.read_word(a)) << 16)
            | u32::from(amiga.memory.read_word(a.wrapping_add(2)))
    };
    let rw = |amiga: &Amiga, a: u32| amiga.memory.read_word(a);
    let _rb = |amiga: &Amiga, a: u32| amiga.memory.read_byte(a);

    let total_ticks = 600u64 * ccks_per_frame;
    let mut triggered = false;
    let mut window_ticks_left: u64 = 0;
    let mut pc_log: Vec<(u64, u32, u32, u32)> = Vec::new(); // (tick, pc, a0, a1)
    let mut prev_pc: u32 = 0;
    let mut hit_begin_io = 0u32;
    let hit_putmsg = 0u32;
    let hit_signal_entry = 0u32;
    let mut hit_return = 0u32;
    let mut floppy_events: Vec<(u64, String)> = Vec::new();
    let mut prev_selected = false;
    let mut prev_motor_on = false;
    let mut prev_cyl: u32 = 0;
    let mut prev_intreq = 0u16;
    let mut trackdisk_pc_hits = 0u64;
    let mut exec_pc_hits = 0u64;
    let mut irq_events: Vec<(u64, u16)> = Vec::new();
    let mut ts3_samples: u64 = 0;
    let mut last_tds_bucket: u64 = 0;
    let mut stop_hits = 0u64;
    let mut pc_hist: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
    let mut vertb_rises = 0u64;
    let mut vertb_clears = 0u64;
    let mut prev_vertb_set = false;
    let mut io_request_ptr: u32 = 0;
    let mut io_device: u32 = 0;
    let mut io_unit: u32 = 0;

    // Common candidate addresses inside Kickstart 1.3 (empirically
    // verified during earlier tracing; exec jumps through the lib vector
    // base + negative offsets into actual function addresses).
    //
    // We don't know the exact implementation address; instead we watch
    // the JMP tables and count transitions to function prologues.

    for tick in 0..total_ticks {
        amiga.tick_cck();

        let pc = amiga.cpu.instr_start_pc;
        if pc != prev_pc {
            if !triggered && pc == 0x00FE859C {
                triggered = true;
                window_ticks_left = 2_000_000;
                let a1 = amiga.cpu.regs.a[1];
                io_request_ptr = a1;
                io_device = rl(&amiga, a1.wrapping_add(0x14));
                io_unit = rl(&amiga, a1.wrapping_add(0x18));
                let io_cmd = rw(&amiga, a1.wrapping_add(0x1C));
                let io_flags = _rb(&amiga, a1.wrapping_add(0x1E));
                let io_offset = rl(&amiga, a1.wrapping_add(0x2C));
                let io_length = rl(&amiga, a1.wrapping_add(0x24));
                let io_data = rl(&amiga, a1.wrapping_add(0x28));
                println!("DoIO at tick {tick}:");
                println!("  IORequest @ ${a1:08X}");
                println!("  io_Device = ${io_device:08X}");
                println!("  io_Unit   = ${io_unit:08X}");
                println!("  io_Command= ${io_cmd:04X}");
                println!("  io_Flags  = ${io_flags:02X}");
                println!("  io_Length = ${io_length:08X}");
                println!("  io_Data   = ${io_data:08X}");
                println!("  io_Offset = ${io_offset:08X}");
                println!("  A6(ExecBase) = ${:08X}", amiga.cpu.regs.a[6]);

                // Also dump unit's port fields (if valid pointer).
                if io_unit >= 0x400 && io_unit < 0x0100_0000 {
                    // MsgPort: mp_Node (14), mp_Flags (1), mp_SigBit (1),
                    // mp_SigTask (4), mp_MsgList (12)
                    let port_flags = _rb(&amiga, io_unit.wrapping_add(0x0E));
                    let port_sigbit = _rb(&amiga, io_unit.wrapping_add(0x0F));
                    let port_sigtask = rl(&amiga, io_unit.wrapping_add(0x10));
                    println!("  Unit as MsgPort:");
                    println!("    mp_Flags   = ${port_flags:02X}");
                    println!("    mp_SigBit  = {port_sigbit}");
                    println!("    mp_SigTask = ${port_sigtask:08X}");
                }
            }

            if triggered {
                if pc == 0x00FE9C3E {
                    hit_begin_io += 1;
                }
                if pc == 0x00FE85A0 {
                    hit_return += 1;
                    println!("RETURN to $FE85A0 (DoIO returned!) at tick {tick}");
                }
                // Track JSR target after LVOPutMsg ($FFFFFE92 = -$16E).
                // We can't know impl address without decoding; instead,
                // watch the generic region $FE1000-$FE2000 where Signal
                // and PutMsg typically live in Kickstart 1.3.

                if window_ticks_left > 0 && pc_log.len() < 100 {
                    // Only record "interesting" PCs: ones in trackdisk
                    // region or in exec core.
                    let is_trackdisk = (0x00FE9C00..0x00FE9E00).contains(&pc);
                    let is_exec = (0x00FC0000..0x00FC4000).contains(&pc);
                    let is_strap = (0x00FE8000..0x00FE8800).contains(&pc);
                    if is_trackdisk || is_strap {
                        pc_log.push((tick, pc,
                            amiga.cpu.regs.a[0], amiga.cpu.regs.a[1]));
                    }
                    let _ = is_exec;
                }
            }
            prev_pc = pc;
        }

        if triggered {
            if window_ticks_left > 0 { window_ticks_left -= 1; }
        }

        if triggered {
            // Count PC hits in trackdisk region and exec core.
            let pc = amiga.cpu.instr_start_pc;
            if (0x00FE9C00..0x00FEA400).contains(&pc) {
                trackdisk_pc_hits += 1;
            }
            if (0x00FC0000..0x00FC4000).contains(&pc) {
                exec_pc_hits += 1;
                if pc != prev_pc {
                    // Only count distinct instruction dispatches.
                }
            }
            if pc == 0x00FC0F90 { stop_hits += 1; }

            // PC histogram sampled every 1000 ticks to keep memory bounded.
            if tick % 1000 == 0 {
                *pc_hist.entry(pc).or_insert(0) += 1;
            }

            // Track VERTB rise/fall.
            let vertb_now = (amiga.paula.intreq & 0x0020) != 0;
            if vertb_now && !prev_vertb_set { vertb_rises += 1; }
            if !vertb_now && prev_vertb_set { vertb_clears += 1; }
            prev_vertb_set = vertb_now;

            // Track INTREQ bits of interest
            let iq = amiga.paula.intreq;
            let ie = amiga.paula.intena;
            let bucket = tick / (50 * ccks_per_frame);
            if bucket != last_tds_bucket {
                irq_events.push((tick, iq & ie));
                last_tds_bucket = bucket;
            }
            ts3_samples += 1;
        }

        if triggered {
            let sel = amiga.floppy.selected();
            let mot = amiga.floppy.motor_on();
            let cyl = amiga.floppy.cylinder();
            let iq = amiga.paula.intreq;
            if sel != prev_selected {
                floppy_events.push((tick, format!("selected -> {sel}")));
                prev_selected = sel;
            }
            if mot != prev_motor_on {
                floppy_events.push((tick, format!("motor_on -> {mot}")));
                prev_motor_on = mot;
            }
            if cyl != prev_cyl {
                floppy_events.push((tick, format!("cyl -> {cyl}")));
                prev_cyl = cyl;
            }
            let iq_rise = (iq & !prev_intreq) & 0x0002; // DSKBLK bit
            if iq_rise != 0 {
                floppy_events.push((tick, format!("INTREQ DSKBLK raised (intreq=${iq:04X})")));
            }
            let iq_fall = (!iq & prev_intreq) & 0x0002;
            if iq_fall != 0 {
                floppy_events.push((tick, format!("INTREQ DSKBLK cleared (intreq=${iq:04X})")));
            }
            prev_intreq = iq;
        }
    }

    println!("\nHits: begin_io={hit_begin_io} putmsg={hit_putmsg} signal_entry={hit_signal_entry} return_to_DoIO_site={hit_return}");
    println!("IORequest=${io_request_ptr:08X}  device=${io_device:08X}  unit=${io_unit:08X}");
    println!("\nFirst 100 interesting PCs after DoIO:");
    for (tick, pc, a0, a1) in pc_log.iter().take(100) {
        println!("  tick={tick} PC=${pc:08X} A0=${a0:08X} A1=${a1:08X}");
    }
    println!("\nFloppy events after DoIO (total {}):", floppy_events.len());
    for (tick, evt) in floppy_events.iter().take(40) {
        println!("  tick={tick} {evt}");
    }
    if floppy_events.len() > 80 {
        println!("  ... (showing last 40)");
        for (tick, evt) in floppy_events.iter().rev().take(40).rev() {
            println!("  tick={tick} {evt}");
        }
    }

    println!("\nFinal CPU PC=${:08X} SR=${:04X}", amiga.cpu.instr_start_pc, amiga.cpu.regs.sr);
    println!("Final floppy: selected={} motor_on={} cyl={}",
        amiga.floppy.selected(), amiga.floppy.motor_on(),
        amiga.floppy.cylinder());
    println!("\nPC-hit summary (post-DoIO, {ts3_samples} samples):");
    println!("  trackdisk region ($FE9C00-$FEA400): {trackdisk_pc_hits}");
    println!("  exec core ($FC0000-$FC4000): {exec_pc_hits}");
    println!("  STOP at $FC0F90: {stop_hits}");
    println!("  VERTB INTREQ rises: {vertb_rises}");
    println!("  VERTB INTREQ clears: {vertb_clears}");

    // Print top 20 PCs by sampled count.
    let mut sorted: Vec<(u32, u64)> = pc_hist.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    println!("\nTop 20 sampled PCs (every 1000 ticks):");
    for (pc, count) in sorted.iter().take(20) {
        println!("  ${pc:08X} = {count}");
    }
    println!("\nIRQ (INTREQ & INTENA) buckets (every 50 frames):");
    for (tick, m) in irq_events.iter().take(30) {
        println!("  tick={tick} pending_masked=${m:04X}");
    }
}
