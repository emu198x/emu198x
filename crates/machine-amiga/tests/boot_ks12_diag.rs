//! KS 1.2 diagnostic: dump copper list and palette state.

use machine_amiga::{Amiga, AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion};
use std::fs;

#[test]
#[ignore]
fn test_kick12_boot_trace() {
    let rom = match fs::read("../../roms/kick12_33_180_a500_a1000_a2000.rom") {
        Ok(r) => r,
        Err(_) => {
            eprintln!("KS 1.2 ROM not found, skipping");
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

    let total_ticks: u64 = 28_375_160 * 30;
    for i in 0..total_ticks {
        amiga.tick();
        if i >= 2 * 28_375_160 {
            let tod = amiga.cia_a.tod_counter();
            if tod < 0x010000 {
                amiga.cia_a.set_tod_counter(0x010000 | tod);
            }
        }
    }

    println!("DMACON  = ${:04X}", amiga.agnus.dmacon);
    println!("BPLCON0 = ${:04X}", amiga.denise.bplcon0);
    println!("COP1LC  = ${:08X}", amiga.copper.cop1lc);
    println!("COP2LC  = ${:08X}", amiga.copper.cop2lc);
    println!("COP PC  = ${:08X}", amiga.copper.pc);

    // Dump copper list from COP1LC
    let cop1 = amiga.copper.cop1lc;
    println!("\nCopper list at ${:08X}:", cop1);
    let mask = amiga.memory.chip_ram_mask;
    for i in 0..32 {
        let addr = cop1.wrapping_add(i * 4);
        let w1 = (u16::from(amiga.memory.chip_ram[(addr & mask) as usize]) << 8)
            | u16::from(amiga.memory.chip_ram[((addr + 1) & mask) as usize]);
        let w2 = (u16::from(amiga.memory.chip_ram[((addr + 2) & mask) as usize]) << 8)
            | u16::from(amiga.memory.chip_ram[((addr + 3) & mask) as usize]);

        if w1 & 1 == 0 {
            // MOVE
            let reg = w1 & 0x1FE;
            let name = match reg {
                0x180 => "COLOR00",
                0x182 => "COLOR01",
                0x184 => "COLOR02",
                0x186 => "COLOR03",
                0x100 => "BPLCON0",
                0x08E => "DIWSTRT",
                0x090 => "DIWSTOP",
                0x092 => "DDFSTRT",
                0x094 => "DDFSTOP",
                0x0E0 => "BPL1PTH",
                0x0E2 => "BPL1PTL",
                0x096 => "DMACON",
                _ => "",
            };
            println!("  {:3}: MOVE ${:04X} → ${:03X} {}", i, w2, reg, name);
        } else {
            // WAIT/SKIP
            let vp = (w1 >> 8) & 0xFF;
            let hp = (w1 >> 1) & 0x7F;
            let is_skip = w2 & 1 != 0;
            if w1 == 0xFFFF && w2 == 0xFFFE {
                println!("  {:3}: END OF LIST", i);
                break;
            }
            println!(
                "  {:3}: {} V=${:02X} H=${:02X} mask=${:04X}",
                i,
                if is_skip { "SKIP" } else { "WAIT" },
                vp,
                hp,
                w2,
            );
        }
    }

    // Also dump COP2
    let cop2 = amiga.copper.cop2lc;
    println!("\nCopper list 2 at ${:08X}:", cop2);
    for i in 0..20 {
        let addr = cop2.wrapping_add(i * 4);
        let w1 = (u16::from(amiga.memory.chip_ram[(addr & mask) as usize]) << 8)
            | u16::from(amiga.memory.chip_ram[((addr + 1) & mask) as usize]);
        let w2 = (u16::from(amiga.memory.chip_ram[((addr + 2) & mask) as usize]) << 8)
            | u16::from(amiga.memory.chip_ram[((addr + 3) & mask) as usize]);

        if w1 & 1 == 0 {
            let reg = w1 & 0x1FE;
            println!("  {:3}: MOVE ${:04X} → ${:03X}", i, w2, reg);
        } else {
            let vp = (w1 >> 8) & 0xFF;
            let hp = (w1 >> 1) & 0x7F;
            if w1 == 0xFFFF && w2 == 0xFFFE {
                println!("  {:3}: END OF LIST", i);
                break;
            }
            println!(
                "  {:3}: WAIT V=${:02X} H=${:02X} mask=${:04X}",
                i, vp, hp, w2
            );
        }
    }

    println!("\nPalette:");
    for i in 0..4 {
        println!("  COLOR{:02} = ${:03X}", i, amiga.denise.palette[i]);
    }
}
