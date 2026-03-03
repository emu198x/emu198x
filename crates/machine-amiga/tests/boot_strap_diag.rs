//! STRAP diag: find GfxBase and check A6 at crash.

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

    // Run until just before the crash (~3.2s)
    let total_ticks: u64 = (28_375_160.0 * 3.2) as u64;
    for i in 0..total_ticks {
        amiga.tick();
        if i >= 2 * 28_375_160 {
            let tod = amiga.cia_a.tod_counter();
            if tod < 0x010000 {
                amiga.cia_a.set_tod_counter(0x010000 | tod);
            }
        }
    }

    // Read ExecBase from chip_ram[4]
    let read32 = |base: &[u8], addr: usize| -> u32 {
        (u32::from(base[addr]) << 24)
            | (u32::from(base[addr + 1]) << 16)
            | (u32::from(base[addr + 2]) << 8)
            | u32::from(base[addr + 3])
    };
    let read16 = |base: &[u8], addr: usize| -> u16 {
        (u16::from(base[addr]) << 8) | u16::from(base[addr + 1])
    };

    let eb_ptr = read32(&amiga.memory.chip_ram, 4);
    println!("ExecBase = ${:08X}", eb_ptr);

    // Walk the library list from ExecBase+$17A (LibList.lh_Head)
    // to find graphics.library
    let read_mem = |addr: u32| -> u8 {
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
    let read32m = |addr: u32| -> u32 {
        (u32::from(read_mem(addr)) << 24)
            | (u32::from(read_mem(addr + 1)) << 16)
            | (u32::from(read_mem(addr + 2)) << 8)
            | u32::from(read_mem(addr + 3))
    };
    let read16m = |addr: u32| -> u16 {
        (u16::from(read_mem(addr)) << 8) | u16::from(read_mem(addr + 1))
    };

    // LibList is at ExecBase+$17A
    let lib_list = eb_ptr + 0x17A;
    let mut node = read32m(lib_list); // lh_Head
    println!("\nLibrary list walk:");
    for _ in 0..20 {
        let succ = read32m(node);
        if succ == 0 {
            break;
        }
        let name_ptr = read32m(node + 10);
        let mut name = String::new();
        for j in 0..30 {
            let ch = read_mem(name_ptr + j);
            if ch == 0 { break; }
            name.push(ch as char);
        }
        let version = read16m(node + 0x14);
        println!(
            "  ${:08X}: \"{}\" v{} +$22=${:04X}",
            node, name, version,
            read16m(node + 0x22),
        );
        if name == "graphics.library" {
            println!("    >>> GfxBase = ${:08X}", node);
            println!("    +$22 (ActiView high word) = ${:04X}", read16m(node + 0x22));
            println!("    +$22 (long) = ${:08X}", read32m(node + 0x22));
        }
        node = succ;
    }
}
