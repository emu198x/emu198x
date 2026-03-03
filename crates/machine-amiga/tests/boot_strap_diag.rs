//! STRAP hang diagnostic: trace what triggers the first warm restart.

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
        slow_ram_size: 0,
    });

    // Run up to 10s. Track key events:
    // 1. When "HELP" ($48454C50) first appears at address 0
    // 2. When the reset() handler fires (overlay turns on)
    // 3. What PC is at each event
    let total_ticks: u64 = 28_375_160 * 10;
    let mut help_detected = false;
    let mut overlay_was_off = false;
    let mut reset_count = 0u32;
    let mut last_report: u64 = 0;

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

        let pc = amiga.cpu.regs.pc;

        // Track overlay transitions (reset handler sets overlay=true)
        if !amiga.memory.overlay {
            overlay_was_off = true;
        }
        if overlay_was_off && amiga.memory.overlay {
            overlay_was_off = false;
            reset_count += 1;
            let elapsed_s = i as f64 / 28_375_160.0;
            if reset_count <= 3 {
                println!(
                    "[{:.3}s] RESET #{} detected (overlay on): PC=${:08X}",
                    elapsed_s, reset_count, pc,
                );
            }
        }

        // Detect "HELP" at address 0
        if !help_detected
            && amiga.memory.chip_ram[0] == 0x48
            && amiga.memory.chip_ram[1] == 0x45
            && amiga.memory.chip_ram[2] == 0x4C
            && amiga.memory.chip_ram[3] == 0x50
        {
            help_detected = true;
            let elapsed_s = i as f64 / 28_375_160.0;
            let sp = amiga.cpu.regs.a(7) as usize;
            // Read the exception frame from the expansion bus cache
            // (SP is in expansion space $C00000-$DFFFFF)
            let read_exp = |addr: usize| -> u8 {
                if addr >= 0xC0_0000 && addr < 0xE0_0000 {
                    amiga.memory.expansion_bus_cache[addr - 0xC0_0000]
                } else if addr < amiga.memory.chip_ram.len() {
                    amiga.memory.chip_ram[addr]
                } else {
                    0
                }
            };
            // The exec guru handler at $FC2FF0 saves registers to $180,
            // then at $FC300A reads D7 from (SP). SP points to the
            // original exception frame (before the handler pushed).
            // The handler's movem pushed 16 regs, so the original
            // exception frame is at SP + 16*4 = SP + 64.
            let orig_sp = sp;
            let frame_sr = (u16::from(read_exp(orig_sp)) << 8) | u16::from(read_exp(orig_sp + 1));
            let frame_pc = (u32::from(read_exp(orig_sp + 2)) << 24)
                | (u32::from(read_exp(orig_sp + 3)) << 16)
                | (u32::from(read_exp(orig_sp + 4)) << 8)
                | u32::from(read_exp(orig_sp + 5));
            println!(
                "[{:.3}s] GURU MEDITATION! PC=${:08X} SP=${:08X}",
                elapsed_s, pc, amiga.cpu.regs.a(7),
            );
            println!(
                "  Exception frame at ${:06X}: SR=${:04X} PC=${:08X}",
                orig_sp, frame_sr, frame_pc,
            );
            println!(
                "  D7=${:08X} A5=${:08X} A6=${:08X}",
                amiga.cpu.regs.d[7],
                amiga.cpu.regs.a(5),
                amiga.cpu.regs.a(6),
            );
            // Dump chip RAM at the faulting PC
            if frame_pc < 0x80000 {
                let a = frame_pc as usize;
                print!("  Code at ${:06X}:", frame_pc);
                for j in 0..8 {
                    print!(" {:02X}", amiga.memory.chip_ram[a + j]);
                }
                println!();
            }
        }

        // Clear help_detected when address 0 is cleared (for next cycle)
        if help_detected
            && amiga.memory.chip_ram[0] == 0
            && amiga.memory.chip_ram[1] == 0
        {
            help_detected = false;
        }

        // Periodic report
        if i - last_report >= 28_375_160 {
            last_report = i;
            let elapsed_s = i as f64 / 28_375_160.0;
            println!(
                "[{:.0}s] PC=${:08X} DMACON=${:04X} INTENA=${:04X}",
                elapsed_s, pc, amiga.agnus.dmacon, amiga.paula.intena,
            );
        }
    }

    // Check ExecBase fields in expansion cache
    let eb = u32::from(amiga.memory.chip_ram[4]) << 24
        | u32::from(amiga.memory.chip_ram[5]) << 16
        | u32::from(amiga.memory.chip_ram[6]) << 8
        | u32::from(amiga.memory.chip_ram[7]);
    // Read saved registers from $180 (guru handler saves D0-A7 there)
    // D0 at $180, D1 at $184, ..., D7 at $19C, A0 at $1A0, ..., A6 at $1B8
    let read32 = |addr: usize| -> u32 {
        (u32::from(amiga.memory.chip_ram[addr]) << 24)
            | (u32::from(amiga.memory.chip_ram[addr + 1]) << 16)
            | (u32::from(amiga.memory.chip_ram[addr + 2]) << 8)
            | u32::from(amiga.memory.chip_ram[addr + 3])
    };
    let saved_a5 = read32(0x1B4);
    let saved_a6 = read32(0x1B8);
    let saved_d0 = read32(0x180);
    println!("Saved regs at guru: D0=${:08X} A5=${:08X} A6/FP=${:08X}", saved_d0, saved_a5, saved_a6);
    // FP+$22 is the divisor — read it
    if saved_a6 >= 0xC0_0000 && saved_a6 < 0xE0_0000 {
        let fp_off = (saved_a6 - 0xC0_0000) as usize;
        let divisor = (u16::from(amiga.memory.expansion_bus_cache[fp_off + 0x22]) << 8)
            | u16::from(amiga.memory.expansion_bus_cache[fp_off + 0x23]);
        println!("FP+$22 (divisor) = ${:04X}", divisor);
    } else if saved_a6 < 0x80000 {
        let divisor = (u16::from(amiga.memory.chip_ram[saved_a6 as usize + 0x22]) << 8)
            | u16::from(amiga.memory.chip_ram[saved_a6 as usize + 0x23]);
        println!("FP+$22 (divisor) = ${:04X} (chip RAM)", divisor);
    }

    println!("\nExecBase = ${:08X}", eb);
    if eb >= 0xC0_0000 && eb < 0xE0_0000 {
        let cache = &amiga.memory.expansion_bus_cache;
        let off = (eb - 0xC0_0000) as usize;
        println!("  +$22 SoftVer = ${:04X}",
            (u16::from(cache[off + 0x22]) << 8) | u16::from(cache[off + 0x23]));
        println!("  +$26 ChkBase = ${:08X}",
            (u32::from(cache[off + 0x26]) << 24)
            | (u32::from(cache[off + 0x27]) << 16)
            | (u32::from(cache[off + 0x28]) << 8)
            | u32::from(cache[off + 0x29]));
        // Dump first 64 bytes of ExecBase
        print!("  ExecBase dump:");
        for i in 0..64 {
            if i % 16 == 0 { print!("\n    +${:02X}:", i); }
            print!(" {:02X}", cache[off + i]);
        }
        println!();
    }
    // Dump chip RAM at $200 (the FP structure)
    println!("\nStructure at $200 (FP at guru):");
    for row in 0..4 {
        let base = 0x200 + row * 16;
        print!("  ${:04X}:", base);
        for j in 0..16 {
            print!(" {:02X}", amiga.memory.chip_ram[base + j]);
        }
        println!();
    }

    // Also check: what does GfxBase point to?
    // ExecBase->LibList is a doubly-linked list. GfxBase is opened by name.
    // For now, just check if $200 is a Node structure.
    let node_type = amiga.memory.chip_ram[0x200 + 8]; // ln_Type at offset 8
    let node_pri = amiga.memory.chip_ram[0x200 + 9] as i8; // ln_Pri at offset 9
    let name_ptr = read32(0x200 + 10); // ln_Name at offset 10
    println!("  Node: type={} pri={} name_ptr=${:08X}", node_type, node_pri, name_ptr);

    // Read name string if it's in ROM
    if name_ptr >= 0xFC0000 && name_ptr < 0x1000000 {
        let rom_off = (name_ptr & amiga.memory.kickstart_mask) as usize;
        let mut name = String::new();
        for i in 0..30 {
            let ch = amiga.memory.kickstart[rom_off + i];
            if ch == 0 { break; }
            name.push(ch as char);
        }
        println!("  Name: \"{}\"", name);
    }

    println!("\nTotal resets: {}", reset_count);
}
