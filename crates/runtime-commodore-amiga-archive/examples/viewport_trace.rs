//! Trace writes to IntBase.ViewLord.ViewPort ($C03D46) and a wider region
//! covering the whole ViewLord (18 bytes from IntBase+$22).
//! If ANY code ever attempts to write a non-NULL value to ViewLord.ViewPort,
//! we'll see it here (even if dropped by our memory allocator).

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

    // Watch the ViewLord region: $C03D46 (ViewPort) through $C03D58 (end of View struct = 18 bytes).
    amiga.memory.watch_range = Some((0x00C0_3D46, 0x00C0_3D58));

    let ccks_per_frame = u64::from(amiga.agnus.lines_per_frame)
        * u64::from(commodore_agnus_ocs::PAL_CCKS_PER_LINE);

    for _ in 0..(200 * ccks_per_frame) {
        amiga.tick_cck();
    }

    println!("=== All writes to ViewLord region ($C03D46-$C03D57) ===");
    println!("{} writes captured\n", amiga.memory.watch_log.len());
    for (addr, size, val) in &amiga.memory.watch_log {
        let offset = addr.wrapping_sub(0x00C0_3D46);
        let field = match offset {
            0..=3 => "ViewPort",
            4..=7 => "LOFCprList",
            8..=0xB => "SHFCprList",
            0xC..=0xD => "DyOffset",
            0xE..=0xF => "DxOffset",
            0x10..=0x11 => "Modes",
            _ => "?",
        };
        let val_str = match size {
            'b' => format!("${:02X}", val & 0xFF),
            'w' => format!("${:04X}", val & 0xFFFF),
            _ => format!("${val:08X}"),
        };
        println!("  ${addr:08X} (+${offset:02X} = {field}): {size} write {val_str}");
    }
}
