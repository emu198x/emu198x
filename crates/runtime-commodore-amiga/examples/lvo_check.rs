//! Dump bytes at LVO addresses to verify JMP trampolines exist.

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
    for _ in 0..(300 * ccks_per_frame) {
        amiga.tick_cck();
    }

    let rl = |amiga: &Amiga, a: u32| -> u32 {
        (u32::from(amiga.memory.read_word(a)) << 16)
            | u32::from(amiga.memory.read_word(a.wrapping_add(2)))
    };
    let rb = |amiga: &Amiga, a: u32| amiga.memory.read_byte(a);

    let execbase = rl(&amiga, 0x4);
    println!("ExecBase = ${execbase:08X}");

    let lvos = [
        ("Forbid",      0x1E),
        ("Permit",      0x24),
        ("AllocSignal", 0x14A),
        ("SetSignal",   0x132),
        ("SetExcept",   0x138),
        ("Wait",        0x13E),
        ("Signal",      0x144),
        ("AddPort",     0x162),
        ("PutMsg",      0x16E),
        ("GetMsg",      0x174),
        ("ReplyMsg",    0x17A),
        ("WaitPort",    0x180),
        ("AddIntServer",0x0A8),
    ];
    for (name, off) in lvos {
        let addr = execbase.wrapping_sub(off);
        let b0 = rb(&amiga, addr);
        let b1 = rb(&amiga, addr.wrapping_add(1));
        let b2 = rb(&amiga, addr.wrapping_add(2));
        let b3 = rb(&amiga, addr.wrapping_add(3));
        let b4 = rb(&amiga, addr.wrapping_add(4));
        let b5 = rb(&amiga, addr.wrapping_add(5));
        // JMP ABS.L = $4EF9, followed by 4-byte target.
        let target = rl(&amiga, addr.wrapping_add(2));
        println!("  {name:<12} @ ${addr:08X} (-${off:03X}): {b0:02X} {b1:02X} {b2:02X} {b3:02X} {b4:02X} {b5:02X}  → target ${target:08X}");
    }
}
