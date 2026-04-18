//! Dump the raw bootblock bytes from the ADF itself, for comparison
//! with what our emulator decodes into RAM at $1558.

use emu198x_shell::{MediaKind, read_media_asset};
use std::path::Path;

fn main() {
    let disk_path = "/Users/stevehill/Projects/Emu198x-Unclean/Reference/amiga/Operating Systems/Workbench/Workbench v1.3.3 rev 34.34 (1990)(Commodore)(Disk 1 of 2)(Workbench)[Cloanto Amiga Forever Edition].zip";
    let loaded = read_media_asset(Path::new(disk_path), MediaKind::Disk).unwrap();
    let adf = format_commodore_amiga_adf::Adf::from_bytes(loaded.bytes).unwrap();
    let data = adf.data();
    println!("ADF total size: {} bytes", data.len());

    // Bootblock = sectors 0+1 = first 1024 bytes.
    print!("Bootblock header (first 48 bytes):");
    for i in 0..48 {
        if i % 16 == 0 { print!("\n  "); }
        print!(" {:02X}", data[i]);
    }
    println!();

    let magic = &data[0..4];
    let stored_sum = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    let root = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
    println!("\nMagic: {:02X?} ({})", magic, std::str::from_utf8(magic).unwrap_or("(non-ascii)"));
    println!("Stored checksum: ${stored_sum:08X}");
    println!("Root block: {root}");
}
