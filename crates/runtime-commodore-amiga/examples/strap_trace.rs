//! Trace strap execution: detect when it reaches each checkpoint.
//!
//! Strap key PCs (Kickstart 1.3):
//!   $FE8444 — strap init entry
//!   $FE847A — after first AllocMem
//!   $FE8498 — AllocMem succeeded, continuing
//!   $FE84CE — after AllocSignal
//!   $FE8506 — after OpenDevice("trackdisk.device")
//!   $FE8508 — TST.L D0 (D0 = OpenDevice result)
//!   $FE850A — branch path if OpenDevice failed (Alert)
//!   $FE8524 — OpenDevice succeeded path continues
//!   $FE855C — DoIO (CMD_UPDATE)
//!   $FE8570 — DoIO (TD_CHANGESTATE)
//!   $FE859E — DoIO (something else)
//!   $FE85F2 — calls $FE8C9C (bootblock handler?)
//!   $FC0F90 — Exec STOP #$2000 (idle)
//!
//! We hit each of these by polling PC per tick and dropping a marker.

use emu198x_shell::{MediaKind, read_media_asset};
use machine_commodore_amiga::Amiga;
use std::collections::BTreeMap;
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

    let breakpoints: [(u32, &str); 16] = [
        (0x00FE8444, "strap init entry"),
        (0x00FE8498, "AllocMem succeeded"),
        (0x00FE8502, "JSR OpenDevice"),
        (0x00FE8506, "after OpenDevice (TST.L D0)"),
        (0x00FE8524, "OpenDevice OK (continue)"),
        (0x00FE855C, "JSR DoIO CMD_UPDATE"),
        (0x00FE8560, "after DoIO CMD_UPDATE"),
        (0x00FE8570, "JSR DoIO TD_CHANGESTATE"),
        (0x00FE8574, "after DoIO TD_CHANGESTATE (TST.L D0)"),
        (0x00FE8578, "TD_CHANGESTATE OK branch"),
        (0x00FE867C, "TD_CHANGESTATE error branch"),
        (0x00FE859C, "JSR DoIO CMD_READ (bootblock)"),
        (0x00FE85A0, "after DoIO CMD_READ"),
        (0x00FE85F2, "JSR bootblock handler"),
        (0x00FE86EA, "end of strap"),
        (0x00FC0F90, "Exec STOP (idle)"),
    ];

    let bp_map: BTreeMap<u32, &str> = breakpoints.iter().copied().collect();
    let mut hits: BTreeMap<u32, (u64, u64)> = BTreeMap::new();
    let mut prev_pc = u32::MAX;

    let total_ticks = 2000u64 * ccks_per_frame;
    for tick in 0..total_ticks {
        amiga.tick_cck();
        let pc = amiga.cpu.instr_start_pc;
        if pc != prev_pc {
            prev_pc = pc;
            if bp_map.contains_key(&pc) {
                let e = hits.entry(pc).or_insert((0, tick));
                e.0 += 1;
                // keep first-hit tick
            }
        }
    }

    println!("Strap checkpoint hits over 400 frames:");
    for (pc, name) in &breakpoints {
        if let Some((count, first_tick)) = hits.get(pc) {
            println!("  ${pc:08X} ({name}): hits={count}, first tick={first_tick}");
        } else {
            println!("  ${pc:08X} ({name}): NEVER HIT");
        }
    }

    println!(
        "\nFinal PC=${:08X} D0=${:08X} A6=${:08X}",
        amiga.cpu.instr_start_pc,
        amiga.cpu.regs.d[0],
        amiga.cpu.regs.a[6]
    );
}
