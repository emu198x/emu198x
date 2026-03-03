//! KS 1.2 diagnostic: check display state after boot.

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
        slow_ram_size: 0,
    });

    // Run 30 seconds
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
    println!("DIWSTRT = ${:04X}", amiga.agnus.diwstrt);
    println!("DIWSTOP = ${:04X}", amiga.agnus.diwstop);
    println!("DDFSTRT = ${:04X}", amiga.agnus.ddfstrt);
    println!("DDFSTOP = ${:04X}", amiga.agnus.ddfstop);
    println!("Palette:");
    for i in 0..8 {
        println!(
            "  COLOR{:02} = ${:03X}",
            i, amiga.denise.palette[i],
        );
    }
}
