//! Instrument timer.device BeginIO to see if trackdisk ever submits
//! a request to it.
//!
//! Library jump table is negative offsets from library base.
//! For a Device (which Library is an alias of here):
//!   -$06: Open
//!   -$0C: Close
//!   -$12: Expunge
//!   -$18: (reserved)
//!   -$1E: BeginIO
//!   -$24: AbortIO
//!
//! Timer.device library base = $00C022EE.
//! BeginIO trampoline at $00C022EE - $1E = $00C022D0.

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
    let rb = |amiga: &Amiga, a: u32| amiga.memory.read_byte(a);

    // First run far enough that the library exists, so we can read its
    // jump-table trampolines.
    for _ in 0..(250 * ccks_per_frame) {
        amiga.tick_cck();
    }

    let timer_lib_base: u32 = 0x00C022EE;
    // Dump -$06, -$0C, -$12, -$18, -$1E, -$24 trampolines.
    println!("── timer.device library jump table ──");
    for (name, off) in [("Open",0x06u32),("Close",0x0C),("Expunge",0x12),("Reserved",0x18),("BeginIO",0x1E),("AbortIO",0x24)] {
        let addr = timer_lib_base.wrapping_sub(off);
        let opc = u32::from(amiga.memory.read_word(addr)) << 16 | u32::from(amiga.memory.read_word(addr.wrapping_add(2)));
        let tgt = rl(&amiga, addr.wrapping_add(2));
        let b0 = rb(&amiga, addr);
        let b1 = rb(&amiga, addr.wrapping_add(1));
        println!("  {name:<10} @ ${addr:08X}: {b0:02X} {b1:02X} ... raw32=${opc:08X}  target=${tgt:08X}");
    }

    let beginio_trampoline = timer_lib_base.wrapping_sub(0x1E);
    // After the JMP trampoline, the CPU actually starts executing at the
    // target. We watch BOTH the trampoline hit AND the target hit, plus
    // interesting args.
    let target_addr = rl(&amiga, beginio_trampoline.wrapping_add(2));
    println!("\nWatching BeginIO trampoline ${beginio_trampoline:08X} and target ${target_addr:08X}");

    let execbase: u32 = 0x00C00276;
    let putmsg_lvo = execbase.wrapping_sub(0x16E);
    let signal_lvo = execbase.wrapping_sub(0x144);

    let mut trampoline_hits = 0u64;
    let mut target_hits = 0u64;
    let mut unique_callers: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
    let mut iorequests_submitted: Vec<(u64, u32, u16, u32, u32)> = Vec::new();
    let mut signals_to_any: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
    let mut putmsg_count = 0u64;
    let mut prev_tramp = false;

    // Instrument from tick 0 for 500 frames.
    let mut amiga2 = Amiga::new_with_slow_ram(
        fs::read("/Users/stevehill/.emu198x/roms/commodore-amiga/kick13.rom").unwrap(),
        512 * 1024,
    );
    let adf2 = format_commodore_amiga_adf::Adf::from_bytes(
        read_media_asset(Path::new(disk_path), MediaKind::Disk).unwrap().bytes,
    ).unwrap();
    amiga2.insert_disk(adf2);
    amiga2.floppy.acknowledge_disk_change();

    for tick in 0..(500 * ccks_per_frame) {
        amiga2.tick_cck();
        let pc = amiga2.cpu.instr_start_pc;

        if pc == beginio_trampoline {
            if !prev_tramp {
                trampoline_hits += 1;
                // Read the caller's return address from SP.
                let sp = amiga2.cpu.regs.a(7);
                let caller_ret = u32::from(amiga2.memory.read_word(sp)) << 16
                    | u32::from(amiga2.memory.read_word(sp.wrapping_add(2)));
                *unique_callers.entry(caller_ret).or_insert(0) += 1;

                // A1 holds the IORequest. Log its key fields.
                let a1 = amiga2.cpu.regs.a[1];
                let io_unit = u32::from(amiga2.memory.read_word(a1.wrapping_add(0x18))) << 16
                    | u32::from(amiga2.memory.read_word(a1.wrapping_add(0x1A)));
                let io_cmd = amiga2.memory.read_word(a1.wrapping_add(0x1C));
                let io_secs = u32::from(amiga2.memory.read_word(a1.wrapping_add(0x20))) << 16
                    | u32::from(amiga2.memory.read_word(a1.wrapping_add(0x22)));
                let io_micros = u32::from(amiga2.memory.read_word(a1.wrapping_add(0x24))) << 16
                    | u32::from(amiga2.memory.read_word(a1.wrapping_add(0x26)));
                iorequests_submitted.push((tick, a1, io_cmd, io_secs, io_micros));
                let _ = io_unit;
            }
            prev_tramp = true;
        } else {
            prev_tramp = false;
        }

        if pc == target_addr {
            target_hits += 1;
        }

        if pc == signal_lvo {
            let t = amiga2.cpu.regs.a[1];
            *signals_to_any.entry(t).or_insert(0) += 1;
        }

        if pc == putmsg_lvo {
            putmsg_count += 1;
        }
    }

    println!("\n── Results over 500 frames ──");
    println!("timer.device BeginIO trampoline hits: {trampoline_hits}");
    println!("timer.device BeginIO target hits:     {target_hits}");
    println!("PutMsg total: {putmsg_count}");

    println!("\nIORequests submitted to timer.device (first 20):");
    for (tick, req, cmd, secs, micros) in iorequests_submitted.iter().take(20) {
        // Decode known timer commands:
        //   TR_ADDREQUEST = $09 (IOStdReq-derived with UNIT_* sub).
        //   TR_GETSYSTIME = $0A
        //   TR_SETSYSTIME = $0B
        println!(
            "  tick={tick} req=${req:08X} cmd=${cmd:04X} TR_TIME={{sec={secs} us={micros}}}"
        );
    }

    println!("\nUnique BeginIO callers (return addresses):");
    let mut callers: Vec<(u32,u64)> = unique_callers.into_iter().collect();
    callers.sort_by(|a,b| b.1.cmp(&a.1));
    for (addr, n) in callers.iter().take(15) {
        println!("  caller ret=${addr:08X} count={n}");
    }

    println!("\nTop Signal() targets (over 500 frames):");
    let trackdisk_task: u32 = 0x00C0485E;
    let mut sigs: Vec<(u32,u64)> = signals_to_any.into_iter().collect();
    sigs.sort_by(|a,b| b.1.cmp(&a.1));
    for (t, n) in sigs.iter().take(20) {
        let mark = if *t == trackdisk_task { " ← trackdisk" } else { "" };
        println!("  task=${t:08X}  n={n}{mark}");
    }
}
