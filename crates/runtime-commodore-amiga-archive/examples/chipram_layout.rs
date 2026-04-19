//! Dump chip RAM layout after Amiga boot, focusing on where gb_copinit
//! and gb_LOFlist point.

use machine_commodore_amiga::Amiga;
use std::fs;

fn main() {
    let kickstart = fs::read("/Users/stevehill/.emu198x/roms/commodore-amiga/kick13.rom")
        .expect("read kickstart");
    let mut amiga = Amiga::new_with_slow_ram(kickstart, 512 * 1024);
    // Pass "disk" as arg to insert a disk for comparison.
    if std::env::args().any(|a| a == "disk") {
        use emu198x_shell::{MediaKind, read_media_asset};
        let disk_path = "/Users/stevehill/Projects/Emu198x-Unclean/Reference/amiga/Operating Systems/Workbench/Workbench v1.3.3 rev 34.34 (1990)(Commodore)(Disk 1 of 2)(Workbench)[Cloanto Amiga Forever Edition].zip";
        let loaded = read_media_asset(std::path::Path::new(disk_path), MediaKind::Disk).unwrap();
        let adf = format_commodore_amiga_adf::Adf::from_bytes(loaded.bytes).unwrap();
        amiga.insert_disk(adf);
        amiga.floppy.acknowledge_disk_change();
        eprintln!("[with disk inserted]");
    } else {
        eprintln!("[no disk]");
    }

    let ccks_per_frame = u64::from(amiga.agnus.lines_per_frame)
        * u64::from(commodore_agnus_ocs::PAL_CCKS_PER_LINE);
    for _ in 0..(500 * ccks_per_frame) {
        amiga.tick_cck();
    }

    let read_long = |amiga: &Amiga, a: u32| -> u32 {
        (u32::from(amiga.memory.read_word(a)) << 16)
            | u32::from(amiga.memory.read_word(a.wrapping_add(2)))
    };

    let exec_base = read_long(&amiga, 0x4);
    println!("ExecBase = ${:08X}", exec_base);
    println!("cop1lc = ${:08X}", amiga.copper.cop1lc);
    println!("cop2lc = ${:08X}", amiga.copper.cop2lc);
    println!("copper.pc = ${:08X}", amiga.copper.pc);
    println!(
        "bplcon0 (Agnus=${:04X} Denise=${:04X})  dmacon=${:04X}  intena=${:04X}  intreq=${:04X}",
        amiga.agnus.bplcon0,
        amiga.denise.bplcon0,
        amiga.agnus.dmacon,
        amiga.paula.intena,
        amiga.paula.intreq,
    );
    println!(
        "palette[0..8] = [${:03X} ${:03X} ${:03X} ${:03X} ${:03X} ${:03X} ${:03X} ${:03X}]",
        amiga.denise.palette[0],
        amiga.denise.palette[1],
        amiga.denise.palette[2],
        amiga.denise.palette[3],
        amiga.denise.palette[4],
        amiga.denise.palette[5],
        amiga.denise.palette[6],
        amiga.denise.palette[7],
    );

    // Try to find GfxBase via ExecBase.LibList at offset ~$166 in KS1.3
    // (varies). We already know the known-interesting chip RAM range is
    // $0-$1000 where copinit + LOFlist live.

    println!("\nChip RAM $0400-$0C7F (the copinit + 2KB block):");
    for i in 0..(0x880 / 4) {
        let a = 0x400u32 + i * 4;
        let v = read_long(&amiga, a);
        if i % 8 == 0 {
            print!("{:04X}:", a);
        }
        print!(" {:08X}", v);
        if i % 8 == 7 {
            println!();
        }
    }

    // Dump cop2lc area — wide enough to find the end marker.
    let c2lc = amiga.copper.cop2lc;
    println!("\nChip RAM around cop2lc=${c2lc:08X} (512 bytes):");
    for i in 0..128 {
        let a = c2lc.wrapping_add(i * 4);
        let v = read_long(&amiga, a);
        if i % 4 == 0 {
            print!("{:08X}:", a);
        }
        print!(" {:08X}", v);
        if i % 4 == 3 {
            println!();
        }
    }

    // Highlight the gb_LOFlist target: copinit at $420, LOFlist at $420+$A0 = $4C0.
    println!("\n$0480-$0C7F (2KB seq 41 block, 128 long-words sample every 16 bytes):");
    for i in 0..128 {
        let a = 0x480u32 + i * 16;
        if a > 0xC7F {
            break;
        }
        let v0 = read_long(&amiga, a);
        let v1 = read_long(&amiga, a + 4);
        let v2 = read_long(&amiga, a + 8);
        let v3 = read_long(&amiga, a + 12);
        if v0 == 0 && v1 == 0 && v2 == 0 && v3 == 0 {
            continue;
        }
        println!("  {:04X}: {:08X} {:08X} {:08X} {:08X}", a, v0, v1, v2, v3);
    }

    // Explicitly dump at $4C0 area (gb_LOFlist).
    println!("\n$04C0-$04FF (where gb_LOFlist points — should have copper list):");
    for i in 0..16 {
        let a = 0x4C0u32 + i * 4;
        let v = read_long(&amiga, a);
        if i % 4 == 0 {
            print!("{:04X}:", a);
        }
        print!(" {:08X}", v);
        if i % 4 == 3 {
            println!();
        }
    }
}
