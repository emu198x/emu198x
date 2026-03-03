//! STRAP diag: track resident module init progress.

use machine_amiga::{Amiga, AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion};
use std::fs;

#[test]
#[ignore]
fn test_strap_hang() {
    let rom = match fs::read("../../roms/kick13.rom") {
        Ok(r) => r,
        Err(_) => {
            eprintln!("KS 1.3 ROM not found, skipping");
            return;
        }
    };

    let mut amiga = Amiga::new_with_config(AmigaConfig {
        model: AmigaModel::A500,
        chipset: AmigaChipset::Ocs,
        region: AmigaRegion::Pal,
        kickstart: rom,
        slow_ram_size: 512 * 1024,
    });

    // Run 4 seconds, tracking the library list size every 0.1s
    let total_ticks: u64 = 28_375_160 * 4;
    let mut last_report: u64 = 0;

    let read_mem = |amiga: &Amiga, addr: u32| -> u8 {
        if addr < 0x80000 {
            amiga.memory.chip_ram[addr as usize]
        } else if addr >= 0xC0_0000 && addr < 0xE0_0000 {
            let off = ((addr - 0xC0_0000) & amiga.memory.slow_ram_mask) as usize;
            amiga.memory.slow_ram[off]
        } else if addr >= 0xFC0000 {
            amiga.memory.kickstart[(addr & amiga.memory.kickstart_mask) as usize]
        } else {
            0
        }
    };
    let read32m = |amiga: &Amiga, addr: u32| -> u32 {
        (u32::from(read_mem(amiga, addr)) << 24)
            | (u32::from(read_mem(amiga, addr + 1)) << 16)
            | (u32::from(read_mem(amiga, addr + 2)) << 8)
            | u32::from(read_mem(amiga, addr + 3))
    };

    for i in 0..total_ticks {
        amiga.tick();
        if i >= 2 * 28_375_160 {
            let tod = amiga.cia_a.tod_counter();
            if tod < 0x010000 {
                amiga.cia_a.set_tod_counter(0x010000 | tod);
            }
        }

        if i - last_report < 28_375_160 / 10 {
            continue;
        }
        last_report = i;

        let elapsed_s = i as f64 / 28_375_160.0;
        let eb = read32m(&amiga, 4);
        if eb == 0 || eb >= 0xF00000 {
            continue; // ExecBase not set up yet
        }

        // Count libraries in the list
        let lib_list = eb + 0x17A;
        let mut node = read32m(&amiga, lib_list);
        let mut count = 0;
        let mut last_name = String::new();
        for _ in 0..50 {
            let succ = read32m(&amiga, node);
            if succ == 0 { break; }
            let name_ptr = read32m(&amiga, node + 10);
            let mut name = String::new();
            if name_ptr > 0 && name_ptr < 0x1000000 {
                for j in 0..30 {
                    let ch = read_mem(&amiga, name_ptr + j);
                    if ch == 0 { break; }
                    name.push(ch as char);
                }
            }
            last_name = name;
            count += 1;
            node = succ;
        }

        println!(
            "[{:.1}s] PC=${:08X} libs={} last=\"{}\" DMACON=${:04X}",
            elapsed_s,
            amiga.cpu.regs.pc,
            count,
            last_name,
            amiga.agnus.dmacon,
        );
    }
}
