//! STRAP diag: check GfxBase fields at crash time.

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
            ide_disk: None,
            scsi_disk: None,
    });

    // Run 3.35 seconds — right at the crash
    let total_ticks: u64 = (28_375_160.0 * 3.35) as u64;
    for _i in 0..total_ticks {
        amiga.tick();
        // Battclock disabled for testing
        // if i >= 2 * 28_375_160 {
        //     let tod = amiga.cia_a.tod_counter();
        //     if tod < 0x010000 {
        //         amiga.cia_a.set_tod_counter(0x010000 | tod);
        //     }
        // }
    }

    let read_mem = |addr: u32| -> u8 {
        if (addr as usize) < amiga.memory.chip_ram.len() {
            amiga.memory.chip_ram[addr as usize]
        } else if (0xC0_0000..0xE0_0000).contains(&addr) {
            let off = ((addr - 0xC0_0000) & amiga.memory.slow_ram_mask) as usize;
            amiga.memory.slow_ram[off]
        } else if addr >= 0xFC0000 {
            amiga.memory.kickstart[(addr & amiga.memory.kickstart_mask) as usize]
        } else {
            0
        }
    };
    let read32m = |addr: u32| -> u32 {
        (u32::from(read_mem(addr)) << 24)
            | (u32::from(read_mem(addr + 1)) << 16)
            | (u32::from(read_mem(addr + 2)) << 8)
            | u32::from(read_mem(addr + 3))
    };
    let read16m =
        |addr: u32| -> u16 { (u16::from(read_mem(addr)) << 8) | u16::from(read_mem(addr + 1)) };

    // Find graphics.library in the list
    let eb = read32m(4);
    let lib_list = eb + 0x17A;
    let mut node = read32m(lib_list);
    println!("ExecBase = ${:08X}", eb);
    for _ in 0..50 {
        let succ = read32m(node);
        if succ == 0 {
            break;
        }
        let name_ptr = read32m(node + 10);
        let mut name = String::new();
        if name_ptr > 0 && name_ptr < 0x1000000 {
            for j in 0..30u32 {
                let ch = read_mem(name_ptr + j);
                if ch == 0 {
                    break;
                }
                name.push(ch as char);
            }
        }
        let version = read16m(node + 0x14);
        println!("  ${:08X}: \"{}\" v{}", node, name, version);
        if name == "graphics.library" {
            println!("    GfxBase = ${:08X}", node);
            // Dump first 48 bytes of graphics-specific data (after library header)
            println!("    Library header ends at +$22. Graphics data:");
            for row in 0..3 {
                let off = 0x22 + row * 16;
                print!("      +${:02X}:", off);
                for j in 0..16 {
                    print!(" {:02X}", read_mem(node + off as u32 + j as u32));
                }
                println!();
            }
            println!("    +$22 (ActiView) = ${:08X}", read32m(node + 0x22));
            println!("    +$26 (copinit)  = ${:08X}", read32m(node + 0x26));
            println!("    +$2A (cia)      = ${:08X}", read32m(node + 0x2A));
        }
        node = succ;
    }

    // Check FP=$C022EE (the actual A6 at the DIVU)
    let fp: u32 = 0xC022EE;
    let fp_off = ((fp - 0xC0_0000) & amiga.memory.slow_ram_mask) as usize;
    println!(
        "\nStructure at FP=${:08X} (slow_ram offset ${:X}):",
        fp, fp_off
    );
    for row in 0..4 {
        let off = row * 16;
        print!("  +${:02X}:", off);
        for j in 0..16 {
            print!(" {:02X}", amiga.memory.slow_ram[fp_off + off + j]);
        }
        println!();
    }
    let divisor = read16m(fp + 0x22);
    println!("  +$22 (divisor) = ${:04X} = {}", divisor, divisor);
}
