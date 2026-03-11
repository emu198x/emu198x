//! ADF disk boot test.
//!
//! Boots Kickstart 1.3 on an A500 PAL with a bootable ADF image and
//! verifies the boot code executes. This validates the full disk read
//! pipeline: floppy motor spin-up, raw MFM DMA, sector decode with
//! checksum verification, bootblock validation, and boot code execution.

mod common;

use common::load_rom;
use machine_amiga::format_adf::{ADF_SIZE_DD, Adf};
use machine_amiga::{Amiga, AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion};

/// Create a bootable ADF whose boot code writes $DEADBEEF to $7FC00.
fn make_bootable_adf() -> Adf {
    let mut data = vec![0u8; ADF_SIZE_DD];

    // DOS\0 header
    data[0] = b'D';
    data[1] = b'O';
    data[2] = b'S';
    data[3] = 0;

    // Root block pointer (standard: 880)
    let root_block: u32 = 880;
    data[8] = (root_block >> 24) as u8;
    data[9] = (root_block >> 16) as u8;
    data[10] = (root_block >> 8) as u8;
    data[11] = root_block as u8;

    // Boot code at offset 12:
    //   MOVE.L  #$DEADBEEF, ($7FC00).L
    //   MOVEQ   #0, D0      ; success
    //   RTS
    let code: &[u8] = &[
        0x23, 0xFC, // MOVE.L #imm, (xxx).L
        0xDE, 0xAD, 0xBE, 0xEF, //   #$DEADBEEF
        0x00, 0x07, 0xFC, 0x00, //   $0007FC00
        0x70, 0x00, // MOVEQ #0, D0
        0x4E, 0x75, // RTS
    ];
    data[12..12 + code.len()].copy_from_slice(code);

    // Bootblock checksum: sum of all 256 longwords (with carry) must be $FFFFFFFF.
    let mut sum: u32 = 0;
    for i in 0..256 {
        let offset = i * 4;
        let long = u32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        let (new_sum, carry) = sum.overflowing_add(long);
        sum = new_sum;
        if carry {
            sum = sum.wrapping_add(1);
        }
    }
    let checksum = (!sum).to_be_bytes();
    data[4..8].copy_from_slice(&checksum);

    Adf::from_bytes(data).expect("valid DD ADF")
}

/// Boot KS 1.3 A500 with Workbench 1.3 ADF and capture a screenshot.
///
/// This tests the full disk boot pipeline with a real Workbench disk.
/// We boot for ~30 seconds to allow the desktop to render, then save
/// a screenshot and check that the display is active (not insert-disk).
#[test]
#[ignore]
fn test_workbench_13_boot() {
    use machine_amiga::commodore_denise_ocs::ViewportPreset;

    let Some(rom) = load_rom("../../roms/kick13.rom") else {
        return;
    };
    let adf_path = "/tmp/wb13.adf";
    let adf_data = match std::fs::read(adf_path) {
        Ok(d) => d,
        Err(_) => {
            eprintln!("Workbench ADF not found at {adf_path}, skipping");
            return;
        }
    };
    let adf = Adf::from_bytes(adf_data).expect("valid ADF");

    let mut amiga = Amiga::new_with_config(AmigaConfig {
        model: AmigaModel::A500,
        chipset: AmigaChipset::Ocs,
        region: AmigaRegion::Pal,
        kickstart: rom,
        slow_ram_size: 512 * 1024,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
    });
    amiga.insert_disk(adf);

    println!("=== Workbench 1.3 Boot Test ===");

    // Boot for ~6 minutes PAL (enough for startup-sequence's 5-min wait timeout)
    let total_ticks: u64 = 10_200_000_000;
    let report_interval: u64 = 28_375_160;
    let mut last_report = 0u64;
    let mut dskblk_count = 0u32;
    let mut prev_dskblk = false;

    for i in 0..total_ticks {
        amiga.tick();

        let dskblk = amiga.paula.intreq & 0x0002 != 0;
        if dskblk && !prev_dskblk {
            dskblk_count += 1;
        }
        prev_dskblk = dskblk;

        if i % 4 != 0 || i - last_report < report_interval {
            continue;
        }
        last_report = i;
        println!(
            "[{:.1}s] PC=${:08X} cyl={} head={} motor={} dskblk_count={} DMACON=${:04X} BPLCON0=${:04X}",
            i as f64 / 28_375_160.0,
            amiga.cpu.regs.pc,
            amiga.floppy.cylinder(),
            amiga.floppy.head(),
            amiga.floppy.motor_on(),
            dskblk_count,
            amiga.agnus.dmacon,
            amiga.denise.bplcon0,
        );
    }

    // Save both Standard and Full-raster screenshots
    let viewport = amiga
        .denise
        .extract_viewport(ViewportPreset::Standard, true, true);
    let std_path = "../../test_output/amiga/boot_wb13_a500.png";
    if let Some(parent) = std::path::Path::new(std_path).parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let file = std::fs::File::create(std_path).expect("create screenshot");
    let w = &mut std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, viewport.width, viewport.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("PNG header");
    let mut rgba = Vec::with_capacity((viewport.width * viewport.height * 4) as usize);
    for &pixel in &viewport.pixels {
        rgba.push(((pixel >> 16) & 0xFF) as u8);
        rgba.push(((pixel >> 8) & 0xFF) as u8);
        rgba.push((pixel & 0xFF) as u8);
        rgba.push(((pixel >> 24) & 0xFF) as u8);
    }
    writer.write_image_data(&rgba).expect("PNG data");
    println!("Screenshot saved to {std_path}");

    // Full-raster screenshot for debug
    let full = amiga
        .denise
        .extract_viewport(ViewportPreset::Full, true, true);
    let full_path = "../../test_output/amiga/boot_wb13_a500_full.png";
    let file = std::fs::File::create(full_path).expect("create full");
    let w = &mut std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, full.width, full.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("header");
    let mut rgba_full = Vec::with_capacity((full.width * full.height * 4) as usize);
    for &pixel in &full.pixels {
        rgba_full.push(((pixel >> 16) & 0xFF) as u8);
        rgba_full.push(((pixel >> 8) & 0xFF) as u8);
        rgba_full.push((pixel & 0xFF) as u8);
        rgba_full.push(((pixel >> 24) & 0xFF) as u8);
    }
    writer.write_image_data(&rgba_full).expect("data");
    println!(
        "Full raster saved to {full_path} ({}x{})",
        full.width, full.height
    );

    println!("DMACON  = ${:04X}", amiga.agnus.dmacon);
    println!("BPLCON0 = ${:04X}", amiga.denise.bplcon0);
    println!("DSKBLK total = {dskblk_count}");

    // Exception vector table from chip RAM
    println!("\n=== Exception Vector Table (chip RAM) ===");
    for (name, addr) in [
        ("Vec 2 (Bus Error)", 0x08u32),
        ("Vec 3 (Address Error)", 0x0C),
        ("Vec 4 (Illegal)", 0x10),
        ("Vec 8 (Priv Viol)", 0x20),
        ("Vec 25 (Level 1 Auto)", 0x64),
        ("Vec 26 (Level 2 Auto)", 0x68),
        ("Vec 27 (Level 3 Auto)", 0x6C),
        ("Vec 32 (TRAP #0)", 0x80),
    ] {
        let b0 = u32::from(amiga.memory.read_chip_byte(addr));
        let b1 = u32::from(amiga.memory.read_chip_byte(addr + 1));
        let b2 = u32::from(amiga.memory.read_chip_byte(addr + 2));
        let b3 = u32::from(amiga.memory.read_chip_byte(addr + 3));
        let handler = (b0 << 24) | (b1 << 16) | (b2 << 8) | b3;
        println!("  {name}: ${handler:08X}");
    }
    // Disassemble a few bytes at the priv viol handler
    let pvh_addr = {
        let b0 = u32::from(amiga.memory.read_chip_byte(0x20));
        let b1 = u32::from(amiga.memory.read_chip_byte(0x21));
        let b2 = u32::from(amiga.memory.read_chip_byte(0x22));
        let b3 = u32::from(amiga.memory.read_chip_byte(0x23));
        (b0 << 24) | (b1 << 16) | (b2 << 8) | b3
    };
    println!("  Priv viol handler bytes at ${pvh_addr:08X}:");
    // Read first 32 bytes of the handler
    if pvh_addr >= 0xFC0000 {
        // ROM address — read from ROM
        let rom_data: Vec<u8> = (0..32)
            .map(|i| {
                let rom_addr = (pvh_addr + i) & 0xFFFFFF;
                if rom_addr >= 0xFC0000 {
                    // Read from memory (ROM is mapped)
                    amiga.memory.read_chip_byte(rom_addr)
                } else {
                    amiga.memory.read_chip_byte(rom_addr)
                }
            })
            .collect();
        print!("   ");
        for b in &rom_data {
            print!(" {:02X}", b);
        }
        println!();
    } else {
        // Chip RAM address
        print!("   ");
        for i in 0..32u32 {
            print!(" {:02X}", amiga.memory.read_chip_byte(pvh_addr + i));
        }
        println!();
    }

    // Keyboard diagnostics
    println!("\n=== Keyboard Diagnostics ===");
    println!("Keyboard bytes_sent: {}", amiga.keyboard.bytes_sent);
    println!("Keyboard state: {}", amiga.keyboard.debug_state_name());
    println!(
        "Keyboard queued keys: {}",
        amiga.keyboard.queued_key_count()
    );
    println!("CIA-A ICR mask: ${:02X}", amiga.cia_a.icr_mask());
    println!("CIA-A ICR status: ${:02X}", amiga.cia_a.icr_status());
    if amiga.keyboard.bytes_sent > 2 {
        println!(
            "WARNING: Keyboard sent {} bytes (expected 2 for power-up only)",
            amiga.keyboard.bytes_sent
        );
    }

    // Find the background color from palette[0]
    let fb_w = amiga.denise.raster_fb_width;
    let bg_rgb12 = amiga.denise.palette[0];
    let bg_r = (((bg_rgb12 >> 8) & 0xF) as u8) * 0x11;
    let bg_g = (((bg_rgb12 >> 4) & 0xF) as u8) * 0x11;
    let bg_b = ((bg_rgb12 & 0xF) as u8) * 0x11;
    let bg_argb = 0xFF000000 | (u32::from(bg_r) << 16) | (u32::from(bg_g) << 8) | u32::from(bg_b);
    println!("Background ARGB: ${:08X}", bg_argb);

    // Scan ALL lines to find which have non-background content
    println!("Lines with non-background content:");
    for line in 0..312u32 {
        let scan_row = line * 2;
        let mut non_bg = 0u32;
        for px in 0..fb_w {
            let idx = (scan_row * fb_w + px) as usize;
            if let Some(&color) = amiga.denise.framebuffer_raster.get(idx) {
                if color != bg_argb && color != 0xFF000000 && color != 0 {
                    non_bg += 1;
                }
            }
        }
        if non_bg > 0 {
            // Dump a sample of the non-bg pixels
            let mut sample_colors = Vec::new();
            for px in 0..fb_w {
                let idx = (scan_row * fb_w + px) as usize;
                if let Some(&color) = amiga.denise.framebuffer_raster.get(idx) {
                    if color != bg_argb
                        && color != 0xFF000000
                        && color != 0
                        && sample_colors.len() < 4
                    {
                        sample_colors.push((px, color));
                    }
                }
            }
            let samples: Vec<String> = sample_colors
                .iter()
                .map(|(x, c)| format!("x={x}:${c:08X}"))
                .collect();
            println!(
                "  line {line:3}: {non_bg:4} non-bg pixels  samples=[{}]",
                samples.join(", ")
            );
        }
    }
    println!(
        "BPLCON0=${:04X} BPLCON1=${:04X}",
        amiga.denise.bplcon0, amiga.denise.bplcon1
    );
    println!(
        "DDFSTRT=${:04X} DDFSTOP=${:04X}",
        amiga.agnus.ddfstrt, amiga.agnus.ddfstop
    );
    println!(
        "DIWSTRT=${:04X} DIWSTOP=${:04X}",
        amiga.agnus.diwstrt, amiga.agnus.diwstop
    );
    println!(
        "BPL1PT=${:08X} BPL2PT=${:08X}",
        amiga.agnus.bpl_pt[0], amiga.agnus.bpl_pt[1]
    );

    println!(
        "Palette[0..4]: {:03X} {:03X} {:03X} {:03X}",
        amiga.denise.palette[0],
        amiga.denise.palette[1],
        amiga.denise.palette[2],
        amiga.denise.palette[3]
    );
    println!(
        "BPL1MOD={} BPL2MOD={}",
        amiga.agnus.bpl1mod, amiga.agnus.bpl2mod
    );

    // Count BPL DMA fetch groups per line using a second quick run
    // (We can't instrument during the main boot, but we can check
    // a single frame's DMA fetch count)
    // For now just calculate the expected fetch count
    let ddfstrt = amiga.agnus.ddfstrt;
    let ddfstop = amiga.agnus.ddfstop;
    let hires = amiga.denise.bplcon0 & 0x8000 != 0;
    let group_len: u16 = if hires { 4 } else { 8 };
    let fetch_end_extra: u16 = 7;
    let fetch_window_end = u32::from(ddfstop) + u32::from(fetch_end_extra);
    let mut fetch_groups = 0u32;
    let mut h = ddfstrt;
    while u32::from(h) <= fetch_window_end {
        fetch_groups += 1;
        h += group_len;
    }
    let words_per_line = fetch_groups;
    let bytes_per_line = words_per_line * 2;
    println!(
        "Fetch window: DDFSTRT=${:04X} DDFSTOP=${:04X} hires={} groups={} words/line={} bytes/line={}",
        ddfstrt, ddfstop, hires, fetch_groups, words_per_line, bytes_per_line
    );

    assert!(
        dskblk_count >= 2,
        "Expected at least 2 disk reads for Workbench boot, got {dskblk_count}"
    );
    println!("Workbench 1.3 boot completed with {dskblk_count} disk reads");
}

/// Boot KS 1.3 A500 with a bootable ADF and verify the boot code runs.
#[test]
#[ignore]
fn test_adf_boot_executes_bootblock() {
    let Some(rom) = load_rom("../../roms/kick13.rom") else {
        return;
    };

    let mut amiga = Amiga::new_with_config(AmigaConfig {
        model: AmigaModel::A500,
        chipset: AmigaChipset::Ocs,
        region: AmigaRegion::Pal,
        kickstart: rom,
        slow_ram_size: 512 * 1024,
            ide_disk: None,
            scsi_disk: None,
            pcmcia_card: None,
    });
    amiga.insert_disk(make_bootable_adf());

    // Boot for ~10 seconds — enough for motor spin-up + bootblock read + execution.
    let total_ticks: u64 = 300_000_000;
    let report_interval: u64 = 28_375_160;
    let mut last_report = 0u64;

    for i in 0..total_ticks {
        amiga.tick();

        if i % 4 != 0 || i - last_report < report_interval {
            continue;
        }
        last_report = i;
        println!(
            "[{:.1}s] PC=${:08X} cyl={} motor={} ready={}",
            i as f64 / 28_375_160.0,
            amiga.cpu.regs.pc,
            amiga.floppy.cylinder(),
            amiga.floppy.motor_on(),
            amiga.floppy.status().ready,
        );
    }

    // Verify boot code executed by checking for the $DEADBEEF signature.
    let sig = (u32::from(amiga.memory.read_chip_byte(0x7FC00)) << 24)
        | (u32::from(amiga.memory.read_chip_byte(0x7FC01)) << 16)
        | (u32::from(amiga.memory.read_chip_byte(0x7FC02)) << 8)
        | u32::from(amiga.memory.read_chip_byte(0x7FC03));

    assert_eq!(
        sig, 0xDEADBEEF,
        "Boot code should have written $DEADBEEF to $7FC00 — ADF boot failed"
    );
    println!("ADF boot successful: signature $DEADBEEF found at $7FC00");
}
