//! Hook the Alert function to capture what alert codes are being raised
//! and who is calling them.

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

    // Resolve Alert LVO: ExecBase - 108 = $FF94.
    // But we don't know when ExecBase is stable. Tick until $4 points into slow or chip RAM
    // and the JMP at ExecBase-108 is valid.
    let rl = |amiga: &Amiga, addr: u32| -> u32 {
        (u32::from(amiga.memory.read_word(addr)) << 16)
            | u32::from(amiga.memory.read_word(addr.wrapping_add(2)))
    };

    let mut alert_lvo: Option<u32> = None;
    let mut prev_pc = u32::MAX;
    let mut hits = 0usize;

    let total_ticks = 500u64 * ccks_per_frame;
    for _tick in 0..total_ticks {
        amiga.tick_cck();

        if alert_lvo.is_none() {
            let exec_base = rl(&amiga, 0x4);
            if (0x400..0x80000).contains(&exec_base) || (0xC0_0000..0xC8_0000).contains(&exec_base) {
                let lvo = exec_base.wrapping_sub(108);
                let jmp_op = amiga.memory.read_word(lvo);
                if jmp_op == 0x4EF9 {
                    alert_lvo = Some(lvo);
                }
            }
        }

        let pc = amiga.cpu.instr_start_pc;
        if pc == prev_pc {
            continue;
        }
        prev_pc = pc;

        if Some(pc) == alert_lvo {
            hits += 1;
            let d7 = amiga.cpu.regs.d[7];
            let sp = amiga.cpu.regs.active_sp();
            let ret = rl(&amiga, sp);
            let a6 = amiga.cpu.regs.a[6];
            if hits <= 10 {
                println!("Alert call #{hits}: D7=${d7:08X}  caller=${ret:08X}  A6=${a6:08X}");
            }
        }
    }

    println!("\nTotal Alert calls: {hits}");
    println!("Alert LVO resolved: {alert_lvo:?}");
}
