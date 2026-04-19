//! Verify strap successfully loaded and decoded the bootblock. The
//! IORequest at DoIO time had io_Data=$1558, io_Length=$400. If MFM
//! decoding works, $1558 should contain "DOS\0" at offset 0 followed
//! by a 32-bit checksum and the boot code.

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

    // strap's DMA completes around tick 14.9M; capture snapshots around
    // that moment to see whether bytes 0..7 are ever correct.
    let snapshots = [14_900_000u64, 15_000_000, 15_500_000, 16_000_000, 18_000_000, 25_000_000, 40_000_000];
    let mut snaps: Vec<(u64, [u8; 16])> = Vec::new();
    let mut cursor = 0u64;
    for target in snapshots {
        while cursor < target {
            amiga.tick_cck();
            cursor += 1;
        }
        let mut bytes = [0u8; 16];
        for i in 0..16u32 {
            bytes[i as usize] = amiga.memory.read_byte(0x00001558 + i);
        }
        snaps.push((cursor, bytes));
    }
    println!("Snapshots of $1558..$1567 over time:");
    for (t, b) in &snaps {
        print!("  tick={t:>10}:");
        for v in b { print!(" {:02X}", v); }
        println!();
    }

    // Compare the RAM bootblock against the ADF's byte-for-byte to see
    // the exact mismatch regions.
    let adf_bb = {
        let adf = format_commodore_amiga_adf::Adf::from_bytes(
            emu198x_shell::read_media_asset(
                std::path::Path::new("/Users/stevehill/Projects/Emu198x-Unclean/Reference/amiga/Operating Systems/Workbench/Workbench v1.3.3 rev 34.34 (1990)(Commodore)(Disk 1 of 2)(Workbench)[Cloanto Amiga Forever Edition].zip"),
                emu198x_shell::MediaKind::Disk,
            ).unwrap().bytes,
        ).unwrap();
        adf.data()[..1024].to_vec()
    };
    let mut ram_bb = vec![0u8; 1024];
    for i in 0..1024u32 {
        ram_bb[i as usize] = amiga.memory.read_byte(0x00001558 + i);
    }
    let mut mismatches: Vec<(usize, u8, u8)> = Vec::new();
    for i in 0..1024 {
        if ram_bb[i] != adf_bb[i] {
            mismatches.push((i, adf_bb[i], ram_bb[i]));
        }
    }
    println!("\nTotal mismatched bytes between RAM and ADF (1024 bytes): {}", mismatches.len());
    println!("First 40 mismatches (offset: adf -> ram):");
    for (o, a, r) in mismatches.iter().take(40) {
        println!("  +${o:03X}: {a:02X} -> {r:02X}");
    }
    // Group by 4-byte (cooked-long) boundaries to see which cooked
    // longs are wrong.
    let mut wrong_longs: Vec<u32> = Vec::new();
    let mut i = 0;
    while i < 1024 {
        let long_start = i & !3;
        let any_diff = (long_start..long_start + 4).any(|k| ram_bb[k] != adf_bb[k]);
        if any_diff {
            wrong_longs.push(long_start as u32);
        }
        i = long_start + 4;
    }
    println!("\nCooked-long mismatches (total {}):", wrong_longs.len());
    for lo in &wrong_longs {
        let expected = u32::from_be_bytes([
            adf_bb[*lo as usize], adf_bb[*lo as usize + 1],
            adf_bb[*lo as usize + 2], adf_bb[*lo as usize + 3],
        ]);
        let got = u32::from_be_bytes([
            ram_bb[*lo as usize], ram_bb[*lo as usize + 1],
            ram_bb[*lo as usize + 2], ram_bb[*lo as usize + 3],
        ]);
        println!("  +${lo:03X} ({}): expected ${expected:08X}  got ${got:08X}  xor=${:08X}",
            if *lo < 512 { "sec0" } else { "sec1" }, expected ^ got);
    }
    // Sector 1 starts at +$200. Dump bytes around that boundary.
    println!("\nRAM +$1F8..+$21F (end of sector 0 / start of sector 1):");
    print!(" ");
    for i in 0x1F8..0x220 {
        if (i - 0x1F8) % 16 == 0 { print!("\n  "); }
        print!(" {:02X}", ram_bb[i]);
    }
    println!();
    println!("ADF +$1F8..+$21F:");
    print!(" ");
    for i in 0x1F8..0x220 {
        if (i - 0x1F8) % 16 == 0 { print!("\n  "); }
        print!(" {:02X}", adf_bb[i]);
    }
    println!();

    // Dump 64 bytes at $1558 and the 1024 bytes as a whole for
    // checksum analysis.
    const BB_ADDR: u32 = 0x0000_1558;

    print!("Bootblock header bytes at ${BB_ADDR:08X}:");
    let mut bb = vec![0u8; 1024];
    for i in 0..1024u32 {
        bb[i as usize] = amiga.memory.read_byte(BB_ADDR + i);
    }
    for i in 0..32 {
        if i % 16 == 0 { print!("\n  "); }
        print!(" {:02X}", bb[i]);
    }
    println!();

    // Check magic.
    let magic = &bb[0..4];
    let is_dos = magic == b"DOS\x00";
    println!("\nMagic bytes: {:02X?} ({})", magic, if is_dos { "DOS\\0 ✓" } else { "INVALID ✗" });

    // The Amiga bootblock checksum algorithm: interpret the bootblock as
    // 256 big-endian u32s. Sum them with carry-around-add; the bootblock
    // is valid if the sum is $FFFFFFFF. The checksum word lives at
    // offset 4 (second u32). During summing, treat that word as 0.
    let mut sum: u32 = 0;
    for chunk in bb.chunks_exact(4) {
        let w = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        let (s, carry) = sum.overflowing_add(w);
        sum = s + u32::from(carry);
    }
    let stored_sum = u32::from_be_bytes([bb[4], bb[5], bb[6], bb[7]]);
    println!("Stored checksum: ${stored_sum:08X}");
    println!("Computed sum:    ${sum:08X} ({})", if sum == 0xFFFFFFFF { "valid ✓" } else { "invalid ✗" });

    let root_block = u32::from_be_bytes([bb[8], bb[9], bb[10], bb[11]]);
    println!("Root block: {root_block} (expected: 880)");

    // Also dump a portion of the code area.
    println!("\nBoot code bytes at +$0C..+$30:");
    print!(" ");
    for i in 12..48 {
        if (i - 12) % 16 == 0 { print!("\n  "); }
        print!(" {:02X}", bb[i]);
    }
    println!();
}
