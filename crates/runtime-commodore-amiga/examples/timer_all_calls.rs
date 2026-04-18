//! Instrument timer.device BeginIO at the TARGET address ($FE9046),
//! catching every entry regardless of whether the caller went through
//! the library trampoline or JSR'd directly.

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

    const BEGINIO_TARGET: u32 = 0x00FE9046;
    const TRACKDISK_TASK: u32 = 0x00C0485E;

    let rl = |amiga: &Amiga, a: u32| -> u32 {
        (u32::from(amiga.memory.read_word(a)) << 16)
            | u32::from(amiga.memory.read_word(a.wrapping_add(2)))
    };

    let mut calls: Vec<(u64, u32, u16, u32, u32, u32, u32, u32)> = Vec::new();
    let mut prev_at = false;

    for tick in 0..(500 * ccks_per_frame) {
        amiga.tick_cck();
        let pc = amiga.cpu.instr_start_pc;
        if pc == BEGINIO_TARGET {
            if !prev_at {
                // A1 = IORequest. Read its IOStdReq fields.
                let a1 = amiga.cpu.regs.a[1];
                let io_flags = amiga.memory.read_byte(a1.wrapping_add(0x1E));
                let io_cmd = amiga.memory.read_word(a1.wrapping_add(0x1C));
                let io_unit = rl(&amiga, a1.wrapping_add(0x18));
                let io_device = rl(&amiga, a1.wrapping_add(0x14));
                // For a TimeRequest, +$20 is tv_secs, +$24 is tv_micros.
                let tv_secs = rl(&amiga, a1.wrapping_add(0x20));
                let tv_mics = rl(&amiga, a1.wrapping_add(0x24));
                // mn_ReplyPort = +$E in Node-based Message — struct Message:
                //   mn_Node  (14)
                //   mn_ReplyPort (4)
                //   mn_Length (2)
                // An IORequest starts with struct Message. So
                //   mn_ReplyPort is at A1+$E (Node is 14 bytes).
                // Actually Node is 14 bytes, so ReplyPort at +$E is
                // correct. But wait - Node is 14 bytes: ln_Succ(4),
                // ln_Pred(4), ln_Type(1), ln_Pri(1), ln_Name(4) = 14.
                // So mn_ReplyPort is at +$E (offset 14).
                let reply_port = rl(&amiga, a1.wrapping_add(0x0E));

                calls.push((tick, a1, io_cmd, io_unit, io_device, tv_secs, tv_mics, reply_port));
                let _ = io_flags;
            }
            prev_at = true;
        } else {
            prev_at = false;
        }
    }

    println!("timer.device BeginIO target entries: {}", calls.len());

    // Group by unique IORequest address to see the pattern.
    let mut distinct: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
    for (_, a1, _, _, _, _, _, _) in &calls {
        *distinct.entry(*a1).or_insert(0) += 1;
    }
    println!("Distinct IORequest addresses: {} ({} total occurrences)", distinct.len(), calls.len());
    for (addr, count) in &distinct {
        println!("  req=${addr:08X} count={count}");
    }

    println!("\nFirst 30 distinct calls (tick, req, cmd, unit, device, tv_sec, tv_us, replyPort):");
    let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
    for (tick, a1, cmd, unit, dev, s, u, rp) in &calls {
        if seen.insert(*a1) {
            println!(
                "  tick={tick} req=${a1:08X} cmd=${cmd:04X} unit=${unit:08X} dev=${dev:08X} tv={{s={s} us={u}}} reply=${rp:08X}"
            );
            if seen.len() >= 30 { break; }
        }
    }

    // Find calls that originate from trackdisk. A heuristic: inspect the
    // reply port for each call — if its mp_SigTask points to trackdisk
    // task, this is a trackdisk timer request.
    println!("\nCalls whose reply port signals trackdisk:");
    let mut trackdisk_calls = 0;
    for (tick, a1, cmd, _unit, _dev, s, u, rp) in &calls {
        if *rp >= 0xC0_0000 && *rp < 0xC8_0000 {
            let sig_task = rl(&amiga, rp.wrapping_add(0x10));
            if sig_task == TRACKDISK_TASK {
                trackdisk_calls += 1;
                if trackdisk_calls <= 20 {
                    let sig_bit = amiga.memory.read_byte(rp.wrapping_add(0x0F));
                    println!(
                        "  tick={tick} req=${a1:08X} cmd=${cmd:04X} tv={{s={s} us={u}}} replyPort=${rp:08X} mp_SigBit={sig_bit}"
                    );
                }
            }
        }
    }
    println!("\nTotal trackdisk timer calls: {trackdisk_calls}");
}
