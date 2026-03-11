//! AGA boot diagnostic: why do A1200/A4000 fail?

use machine_amiga::{Amiga, AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion};
use std::collections::BTreeMap;
use std::fs;

#[test]
#[ignore]
fn test_aga_boot_diag() {
    let rom = match fs::read("../../roms/kick31_40_068_a1200.rom") {
        Ok(r) => r,
        Err(_) => {
            eprintln!("KS 3.1 A1200 ROM not found, skipping");
            return;
        }
    };

    let mut amiga = Amiga::new_with_config(AmigaConfig {
        model: AmigaModel::A1200,
        chipset: AmigaChipset::Aga,
        region: AmigaRegion::Pal,
        kickstart: rom,
        slow_ram_size: 512 * 1024,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
    });

    let total_ticks: u64 = 28_375_160 * 10;
    let mut last_report: u64 = 0;
    let mut pc_histogram: BTreeMap<u32, u64> = BTreeMap::new();
    let trace_start: u64 = 28_375_160 * 5;
    let mut tracing = false;

    let read_mem = |amiga: &Amiga, addr: u32| -> u8 {
        // Use the memory system's read_byte which handles all address ranges
        amiga.memory.read_byte(addr)
    };
    let read32m = |amiga: &Amiga, addr: u32| -> u32 {
        (u32::from(read_mem(amiga, addr)) << 24)
            | (u32::from(read_mem(amiga, addr + 1)) << 16)
            | (u32::from(read_mem(amiga, addr + 2)) << 8)
            | u32::from(read_mem(amiga, addr + 3))
    };

    for i in 0..total_ticks {
        amiga.tick();

        if i % 4 != 0 {
            continue;
        }

        let pc = amiga.cpu.regs.pc;

        if !tracing && i >= trace_start {
            tracing = true;
        }

        if tracing {
            *pc_histogram.entry(pc).or_insert(0) += 1;
        }

        // Check D0 right when the bne is about to evaluate
        if pc == 0xF83196 && !tracing {
            let d0 = amiga.cpu.regs.d[0];
            let sr = amiga.cpu.regs.sr;
            let z = (sr >> 2) & 1;
            let elapsed_s = i as f64 / 28_375_160.0;
            println!(
                "[{:.3}s] at bne: D0=${:08X} SR=${:04X} Z={} IR=${:04X}",
                elapsed_s, d0, sr, z, amiga.cpu.ir,
            );
        }

        // More frequent reports in the first 3 seconds
        let interval = if i < 28_375_160 * 3 {
            28_375_160 / 4
        } else {
            28_375_160
        };
        if i - last_report >= interval {
            last_report = i;
            let elapsed_s = i as f64 / 28_375_160.0;
            let eb = read32m(&amiga, 4);
            println!(
                "[{:.0}s] PC=${:08X} SR=${:04X} DMACON=${:04X} INTENA=${:04X} ExecBase=${:08X}",
                elapsed_s, pc, amiga.cpu.regs.sr, amiga.agnus.dmacon, amiga.paula.intena, eb,
            );
        }
    }

    // Print top 15 hotspots
    let mut hot: Vec<_> = pc_histogram.into_iter().collect();
    hot.sort_by(|a, b| b.1.cmp(&a.1));
    println!("\nTop 15 PC hotspots (5-10s):");
    for (i, (pc, count)) in hot.iter().take(15).enumerate() {
        let label = if *pc >= 0xF80000 {
            format!("ROM+${:05X}", pc - 0xF80000)
        } else {
            format!("chip ${:06X}", pc)
        };
        println!("  {:2}. PC=${:08X} ({}) count={}", i + 1, pc, label, count);
    }

    // Check ExecBase and library list
    let eb = read32m(&amiga, 4);
    println!("\nExecBase = ${:08X}", eb);
    if eb > 0 && eb < 0x200000 {
        let lib_list = eb + 0x17A;
        let mut node = read32m(&amiga, lib_list);
        let mut count = 0;
        println!("Libraries:");
        for _ in 0..30 {
            let succ = read32m(&amiga, node);
            if succ == 0 {
                break;
            }
            let name_ptr = read32m(&amiga, node + 10);
            let mut name = String::new();
            if name_ptr > 0 && name_ptr < 0x1000000 {
                for j in 0..30u32 {
                    let ch = read_mem(&amiga, name_ptr + j);
                    if ch == 0 {
                        break;
                    }
                    name.push(ch as char);
                }
            }
            println!("  ${:08X}: \"{}\"", node, name);
            count += 1;
            node = succ;
        }
        println!("Total: {} libraries", count);
    }
}
