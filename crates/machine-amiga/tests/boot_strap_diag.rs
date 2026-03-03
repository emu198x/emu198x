//! STRAP display hang diagnostic — where does the insert-disk drawing get stuck?
//! Uses KS 1.3 as the reference (best documented).

use machine_amiga::{Amiga, AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion};
use std::collections::BTreeMap;
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
        slow_ram_size: 0,
    });

    // Run 15 seconds — STRAP should be reached by ~10s
    let total_ticks: u64 = 28_375_160 * 15;
    let strap_trace_start: u64 = 28_375_160 * 8; // start tracing at 8s

    let mut tracing = false;
    let mut last_pc: u32 = 0;
    let mut pc_histogram: BTreeMap<u32, u64> = BTreeMap::new();
    let mut last_report: u64 = 0;

    // Track the PC every CPU cycle and build a histogram
    // of where the CPU spends time in the STRAP area
    for i in 0..total_ticks {
        amiga.tick();

        // Battclock
        if i >= 2 * 28_375_160 {
            let tod = amiga.cia_a.tod_counter();
            if tod < 0x010000 {
                amiga.cia_a.set_tod_counter(0x010000 | tod);
            }
        }

        if i % 4 != 0 {
            continue;
        }

        if !tracing && i >= strap_trace_start {
            tracing = true;
        }

        if !tracing {
            continue;
        }

        let pc = amiga.cpu.regs.pc;

        // Count time at each PC
        *pc_histogram.entry(pc).or_insert(0) += 1;

        // Periodic report of current PC
        if i - last_report >= 28_375_160 {
            last_report = i;
            let elapsed_s = i as f64 / 28_375_160.0;
            println!(
                "[{:.0}s] PC=${:08X} SR=${:04X} DMACON=${:04X} BPLCON0=${:04X}",
                elapsed_s,
                pc,
                amiga.cpu.regs.sr,
                amiga.agnus.dmacon,
                amiga.denise.bplcon0,
            );
        }

        last_pc = pc;
    }

    // Print top 20 hotspots
    let mut hot: Vec<_> = pc_histogram.iter().collect();
    hot.sort_by(|a, b| b.1.cmp(a.1));
    println!("\nTop 20 PC hotspots (8-15s):");
    for (i, &(pc, count)) in hot.iter().take(20).enumerate() {
        let (pc, count) = (*pc, *count);
        let rom_off = if pc >= 0xFC0000 {
            format!("ROM+${:05X}", pc - 0xFC0000)
        } else {
            format!("chip ${:06X}", pc)
        };
        println!("  {:2}. PC=${:08X} ({}) count={}", i + 1, pc, rom_off, count);
    }

    println!("\nFinal state:");
    println!("  PC=${:08X}", last_pc);
    println!("  DMACON=${:04X} BPLCON0=${:04X}", amiga.agnus.dmacon, amiga.denise.bplcon0);
    println!("  COP1LC=${:08X} COP2LC=${:08X}", amiga.copper.cop1lc, amiga.copper.cop2lc);
    let mask = amiga.memory.chip_ram_mask;
    let exec_base = u32::from(amiga.memory.chip_ram[4]) << 24
        | u32::from(amiga.memory.chip_ram[5]) << 16
        | u32::from(amiga.memory.chip_ram[6]) << 8
        | u32::from(amiga.memory.chip_ram[7]);
    println!("  ExecBase (addr 4) = ${:08X}", exec_base);
    let addr0 = u32::from(amiga.memory.chip_ram[0]) << 24
        | u32::from(amiga.memory.chip_ram[1]) << 16
        | u32::from(amiga.memory.chip_ram[2]) << 8
        | u32::from(amiga.memory.chip_ram[3]);
    println!("  Address 0 = ${:08X}", addr0);
    println!("  overlay = {}", amiga.memory.overlay);
}
