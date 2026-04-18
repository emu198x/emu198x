//! Find the DMA buffer address (DSKPT) used by strap's disk read,
//! wait for the DMA to complete, then dump the first few hundred
//! bytes of the buffer to see exactly what Paula wrote.

use emu198x_shell::{MediaKind, read_media_asset};
use machine_commodore_amiga::Amiga;
use std::fs;
use std::path::Path;

fn main() {
    let kickstart = fs::read("/Users/stevehill/.emu198x/roms/commodore-amiga/kick13.rom").unwrap();
    let mut amiga = Amiga::new_with_slow_ram(kickstart, 512 * 1024);
    let disk_path = "/Users/stevehill/Projects/Emu198x-Unclean/Reference/amiga/Operating Systems/Workbench/Workbench v1.3.3 rev 34.34 (1990)(Commodore)(Disk 1 of 2)(Workbench)[Cloanto Amiga Forever Edition].zip";
    let loaded = read_media_asset(Path::new(disk_path), MediaKind::Disk).unwrap();
    let adf_bytes = loaded.bytes.clone();
    let adf = format_commodore_amiga_adf::Adf::from_bytes(loaded.bytes).unwrap();
    amiga.insert_disk(adf);
    amiga.floppy.acknowledge_disk_change();

    let ccks_per_frame = u64::from(amiga.agnus.lines_per_frame)
        * u64::from(commodore_agnus_ocs::PAL_CCKS_PER_LINE);

    // Run boot. During trackdisk's first DMA (expected tick ~14.9M),
    // capture DSKPT at the moment DMA is armed (DSKLEN bit 15 set).
    let mut captured_dskpt: Option<u32> = None;
    let mut prev_armed = false;
    let mut captured_at_tick = 0u64;

    let mut last_len = amiga.paula.dsklen;
    for tick in 0..(600 * ccks_per_frame) {
        amiga.tick_cck();
        let armed = amiga.paula.dsklen & 0x8000 != 0;
        if armed && !prev_armed && captured_dskpt.is_none() {
            captured_dskpt = Some(amiga.agnus.dsk_pt);
            captured_at_tick = tick;
            last_len = amiga.paula.dsklen;
            println!("At DMA arm: ADKCON=${:04X}  DSKSYNC=${:04X}  INTENA=${:04X}  DMACON=${:04X}",
                amiga.paula.adkcon, amiga.paula.dsksync, amiga.paula.intena, amiga.agnus.dmacon);
        }
        prev_armed = armed;
    }

    let dskpt = captured_dskpt.unwrap_or(0);
    println!("Captured DSKPT at tick {captured_at_tick}: ${dskpt:08X}  DSKLEN=${last_len:04X}");
    println!("Final DSKPT after transfer: ${:08X}", amiga.agnus.dsk_pt);

    // Dump 64 bytes starting at the captured DSKPT.
    println!("\nFirst 64 bytes of DMA buffer (at captured DSKPT):");
    for row in 0..4 {
        print!("  +${:03X}:", row * 16);
        for col in 0..16 {
            let addr = dskpt + row * 16 + col;
            print!(" {:02X}", amiga.memory.read_byte(addr));
        }
        println!();
    }

    // Also dump the "final DSKPT - 64" area to see tail.
    let tail = amiga.agnus.dsk_pt.saturating_sub(64);
    println!("\nLast 64 bytes of DMA buffer (before final DSKPT=${:08X}):", amiga.agnus.dsk_pt);
    for row in 0..4 {
        print!("  +${:03X}:", row * 16);
        for col in 0..16 {
            let addr = tail + row * 16 + col;
            print!(" {:02X}", amiga.memory.read_byte(addr));
        }
        println!();
    }

    // Compare the first 16 words of the DMA buffer against what the
    // encoder would have produced (after stripping sync). Take track 0
    // sector 0 and encode it, then compare.
    let adf = format_commodore_amiga_adf::Adf::from_bytes(adf_bytes).unwrap();
    let track_data = &adf.data()[..11 * 512];
    let mfm_bytes = peripheral_commodore_amiga_floppy::mfm::encode_mfm_track(track_data, 0, 11);
    // Find first $4489 $4489 (sync pair) in the encoded stream, then
    // skip past it; what follows is sector 0 data.
    let mut idx = 0;
    while idx + 3 < mfm_bytes.len() {
        if mfm_bytes[idx] == 0x44 && mfm_bytes[idx+1] == 0x89 && mfm_bytes[idx+2] == 0x44 && mfm_bytes[idx+3] == 0x89 {
            break;
        }
        idx += 1;
    }
    let post_sync = idx + 4;
    println!("\nEncoder: sync found at byte offset {}; first 64 bytes after sync:", idx);
    for row in 0..4 {
        print!("  +${:03X}:", row * 16);
        for col in 0..16 {
            print!(" {:02X}", mfm_bytes[post_sync + row * 16 + col]);
        }
        println!();
    }
}
