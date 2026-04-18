//! Watch Signal() calls targeting trackdisk.device's task.
//!
//! Exec's _LVOSignal = ExecBase - $138. The JMP-vector at that address
//! trampolines to the implementation. When PC hits it, A1 = target task
//! and D0 = signal mask.

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

    // First, let the system boot far enough that strap can run and
    // locate itself. Then look up the trackdisk task address and the
    // Signal LVO.
    let rl = |amiga: &Amiga, a: u32| -> u32 {
        (u32::from(amiga.memory.read_word(a)) << 16)
            | u32::from(amiga.memory.read_word(a.wrapping_add(2)))
    };

    // These addresses are deterministic from Kickstart 1.3 + 512KB slow.
    let execbase: u32 = 0x00C00276;
    let signal_lvo = execbase.wrapping_sub(0x144);
    let wait_lvo = execbase.wrapping_sub(0x13E);
    let putmsg_lvo = execbase.wrapping_sub(0x16E);
    let replymsg_lvo = execbase.wrapping_sub(0x17A);
    let getmsg_lvo = execbase.wrapping_sub(0x174);
    let trackdisk_task_hard: u32 = 0x00C0485E;

    // trackdisk task from earlier trace is $00C0485E. But fetch it
    // fresh by walking TaskWait until we find name "trackdisk.device".
    let trackdisk_task = trackdisk_task_hard;
    let _ = rl(&amiga, 0x4); // silence warning
    println!("Signal   LVO @ ${signal_lvo:08X}");
    println!("Wait     LVO @ ${wait_lvo:08X}");
    println!("PutMsg   LVO @ ${putmsg_lvo:08X}");
    println!("ReplyMsg LVO @ ${replymsg_lvo:08X}");
    println!("GetMsg   LVO @ ${getmsg_lvo:08X}");
    println!("Trackdisk task = ${trackdisk_task:08X}");
    println!("ExecBase       = ${execbase:08X}");

    let mut signals_to_tdsk: Vec<(u64, u32, u32)> = Vec::new();
    let mut all_signals = 0u64;
    let mut wait_calls = 0u64;
    let mut wait_from_tdsk = 0u64;
    let mut wait_from_tdsk_masks: Vec<(u64, u32)> = Vec::new();
    let mut putmsg_calls = 0u64;
    let mut putmsg_targets: Vec<(u64, u32, u32)> = Vec::new();
    let mut replymsg_calls = 0u64;
    let mut prev_sig_recvd: u32 = !0;
    let mut sig_recvd_changes: Vec<(u64, u32)> = Vec::new();

    // Instrument from tick 0 for 500 frames.
    for tick in 0..(500 * ccks_per_frame) {
        amiga.tick_cck();
        let pc = amiga.cpu.instr_start_pc;
        if pc == signal_lvo {
            all_signals += 1;
            let target = amiga.cpu.regs.a[1];
            let mask = amiga.cpu.regs.d[0];
            if target == trackdisk_task {
                signals_to_tdsk.push((tick, target, mask));
            }
        }
        if pc == wait_lvo {
            wait_calls += 1;
            let tt = rl(&amiga, execbase.wrapping_add(0x114));
            if tt == trackdisk_task {
                wait_from_tdsk += 1;
                if wait_from_tdsk_masks.len() < 30 {
                    let mask = amiga.cpu.regs.d[0];
                    wait_from_tdsk_masks.push((tick, mask));
                }
            }
        }
        if pc == putmsg_lvo {
            putmsg_calls += 1;
            if putmsg_targets.len() < 30 {
                let port = amiga.cpu.regs.a[0];
                let msg = amiga.cpu.regs.a[1];
                putmsg_targets.push((tick, port, msg));
            }
        }
        if pc == replymsg_lvo { replymsg_calls += 1; }
        let _ = getmsg_lvo;

        if trackdisk_task != 0 {
            let sr = rl(&amiga, trackdisk_task.wrapping_add(0x1A));
            if sr != prev_sig_recvd {
                sig_recvd_changes.push((tick, sr));
                prev_sig_recvd = sr;
            }
        }
    }

    println!("\n── LVO call counts ──");
    println!("  Signal   total: {all_signals}");
    println!("  Wait     total: {wait_calls}");
    println!("  Wait from trackdisk: {wait_from_tdsk}");
    println!("  PutMsg:   {putmsg_calls}");
    println!("  ReplyMsg: {replymsg_calls}");

    println!("\nSignal(trackdisk, ...) calls: {}", signals_to_tdsk.len());
    for (tick, task, mask) in signals_to_tdsk.iter().take(30) {
        println!("  tick={tick} task=${task:08X} mask=${mask:08X}");
    }

    println!("\nWait(mask) from trackdisk context (first 30):");
    for (tick, mask) in wait_from_tdsk_masks.iter() {
        println!("  tick={tick} Wait(${mask:08X})");
    }

    println!("\nPutMsg calls (first 30):");
    for (tick, port, msg) in putmsg_targets.iter() {
        println!("  tick={tick} PutMsg(port=${port:08X}, msg=${msg:08X})");
    }

    println!("\ntrackdisk sig_recvd changes ({}):", sig_recvd_changes.len());
    for (tick, sr) in sig_recvd_changes.iter().take(30) {
        println!("  tick={tick} sig_recvd=${sr:08X}");
    }
}
