//! Who signals trackdisk? Capture caller return addresses for every
//! Signal() call targeting trackdisk.

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
    let signal_lvo = execbase.wrapping_sub(0x144);
    let trackdisk_task: u32 = 0x00C0485E;

    let rw = |amiga: &Amiga, a: u32| amiga.memory.read_word(a);
    let rl = |amiga: &Amiga, a: u32| -> u32 {
        (u32::from(rw(amiga, a)) << 16) | u32::from(rw(amiga, a.wrapping_add(2)))
    };

    let mut events: Vec<(u64, u32, u32, u32)> = Vec::new();
    let mut prev_hit = false;

    for tick in 0..(500 * ccks_per_frame) {
        amiga.tick_cck();
        let pc = amiga.cpu.instr_start_pc;
        if pc == signal_lvo {
            if !prev_hit {
                let target = amiga.cpu.regs.a[1];
                if target == trackdisk_task {
                    let mask = amiga.cpu.regs.d[0];
                    // Caller return = top of stack at the time of the
                    // JMP into Signal (JSR already pushed the return
                    // address, and the JMP doesn't push).
                    let sp = amiga.cpu.regs.a(7);
                    let ret = rl(&amiga, sp);
                    events.push((tick, mask, ret, sp));
                }
            }
            prev_hit = true;
        } else {
            prev_hit = false;
        }
    }

    println!("Signal(trackdisk, ...) events: {}", events.len());
    for (tick, mask, ret, sp) in &events {
        println!("  tick={tick} mask=${mask:08X} caller_ret=${ret:08X} SP=${sp:08X}");
    }

    // Group by caller for clarity.
    let mut counts: std::collections::HashMap<(u32,u32), u64> = std::collections::HashMap::new();
    for (_, mask, ret, _) in &events {
        *counts.entry((*ret, *mask)).or_insert(0) += 1;
    }
    println!("\nBy (caller, mask):");
    let mut rows: Vec<((u32,u32),u64)> = counts.into_iter().collect();
    rows.sort_by(|a,b| b.1.cmp(&a.1));
    for ((ret, mask), n) in rows {
        println!("  caller=${ret:08X} mask=${mask:08X}  n={n}");
    }
}
