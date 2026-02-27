//! Code Like It's 198x integration tests.
//!
//! Verify that the Amiga emulator can run actual lesson code from the Code198x
//! curriculum. The Exodus unit-01 is a Copper list landscape that takes over
//! the machine — testing the Copper's ability to change COLOR00 per scanline.

use machine_amiga::commodore_denise_ocs::ViewportPreset;
use machine_amiga::{Amiga, AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion};

const OUTPUT_DIR: &str = "../../test_output";

fn ensure_output_dir() {
    let _ = std::fs::create_dir_all(OUTPUT_DIR);
}

fn load_kickstart() -> Option<Vec<u8>> {
    if let Ok(path) = std::env::var("AMIGA_KS13_ROM") {
        return std::fs::read(&path).ok();
    }
    std::fs::read("../../roms/kick13.rom").ok()
}

fn code198x_path(relative: &str) -> Option<std::path::PathBuf> {
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../Code198x/code-samples")
        .join(relative);
    if base.exists() {
        Some(base)
    } else {
        None
    }
}

/// Extract the CODE hunk from an Amiga hunk executable.
///
/// Returns the raw code bytes (no headers). Supports single-hunk executables.
fn extract_hunk_code(data: &[u8]) -> Option<Vec<u8>> {
    // Hunk format: $000003F3 (HUNK_HEADER), then hunk table, then HUNK_CODE
    if data.len() < 24 {
        return None;
    }
    let magic = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    if magic != 0x0000_03F3 {
        return None; // Not HUNK_HEADER
    }

    // Skip header: names=0, table_size, first, last, then sizes
    // For a simple single-hunk exe: 00 00 03 F3 | 00 00 00 00 (names) |
    //   00 00 00 01 (table_size) | 00 00 00 00 (first) | 00 00 00 00 (last) |
    //   size_in_longs | HUNK_CODE (03E9) | num_longs | data...
    let mut pos = 4;
    // Skip resident library names (terminated by 0 longword)
    let names_count = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
    pos += 4;
    // Skip name strings (each preceded by length in longs)
    for _ in 0..names_count {
        let len = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4 + len * 4;
    }
    // table_size, first, last
    let table_size = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
    pos += 4; // table_size
    pos += 4; // first
    pos += 4; // last
    // Skip hunk sizes
    for _ in 0..table_size {
        pos += 4;
    }
    // Now we should be at HUNK_CODE
    let hunk_type = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
    pos += 4;
    if hunk_type != 0x0000_03E9 {
        return None; // Not HUNK_CODE
    }
    let num_longs = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
    pos += 4;
    let code_bytes = num_longs * 4;
    if pos + code_bytes > data.len() {
        return None;
    }
    Some(data[pos..pos + code_bytes].to_vec())
}

// ---------------------------------------------------------------------------
// Exodus Unit 1: Copper landscape — direct load into chip RAM
//
// The Exodus program disables all DMA/interrupts, installs a Copper list,
// and enables only Copper + master DMA. The Copper writes COLOR00 at various
// beam positions to paint a sky-gradient / earth / underground landscape.
//
// We boot KS1.3 to initialise the hardware, then inject the Exodus code
// directly into chip RAM and jump to it. This bypasses the AmigaDOS disk
// boot path and directly tests Copper rendering of Code198x lesson code.
// ---------------------------------------------------------------------------

#[test]
#[ignore] // Requires KS1.3 ROM and Code198x repo
fn test_exodus_unit01() {
    let exe_path = match code198x_path(
        "commodore-amiga/game-01-exodus/unit-01/exodus",
    ) {
        Some(p) => p,
        None => {
            eprintln!("Skipping: Code198x repo not found");
            return;
        }
    };

    let kickstart = match load_kickstart() {
        Some(ks) => ks,
        None => {
            eprintln!("Skipping: Kickstart ROM not found");
            return;
        }
    };

    ensure_output_dir();

    // Extract raw code from the hunk executable
    let exe_data = std::fs::read(&exe_path).expect("read exodus executable");
    let code = extract_hunk_code(&exe_data).expect("extract hunk code");
    eprintln!("Exodus code: {} bytes", code.len());

    let config = AmigaConfig {
        model: AmigaModel::A500,
        chipset: AmigaChipset::Ocs,
        region: AmigaRegion::Pal,
        kickstart,
    };
    let mut amiga = Amiga::new_with_config(config);

    // The Exodus program takes over the machine completely: disables all
    // DMA and interrupts, installs its own Copper list, enables only
    // Copper + master DMA. It doesn't need the OS at all.
    //
    // Load code into chip RAM, disable the ROM overlay, set CPU state,
    // and let the Exodus init + Copper run.
    let load_addr: u32 = 0x0004_0000;
    for (i, &byte) in code.iter().enumerate() {
        amiga.memory.write_byte(load_addr + i as u32, byte);
    }
    eprintln!(
        "Loaded Exodus code at ${load_addr:08X}-${:08X} ({} bytes)",
        load_addr + code.len() as u32,
        code.len()
    );

    // Disable ROM overlay so chip RAM is accessible at $0
    amiga.memory.overlay = false;

    // Set CPU state: PC at Exodus entry, SSP in fast RAM area
    amiga.cpu.reset_to(0x0007_FF00, load_addr);
    eprintln!("CPU: PC=${:08X} SSP=${:08X}", amiga.cpu.regs.pc, amiga.cpu.regs.ssp);

    // Run frames. Exodus init disables DMA, installs Copper, re-enables.
    // The Copper list runs every frame, writing COLOR00 per scanline.
    for _ in 0..10 {
        amiga.run_frame();
    }
    eprintln!("PC after 10 frames: ${:08X}", amiga.cpu.regs.pc);

    // Extract viewport and count unique colours.
    let pal = matches!(amiga.region, AmigaRegion::Pal);
    let viewport = amiga
        .denise
        .as_inner()
        .extract_viewport(ViewportPreset::Standard, pal, true);

    let mut unique_colors = std::collections::HashSet::new();
    for &pixel in &viewport.pixels {
        unique_colors.insert(pixel);
    }

    eprintln!("Unique colours in viewport: {}", unique_colors.len());
    eprintln!("PC after run: ${:08X}", amiga.cpu.regs.pc);

    // The Copper landscape has 12 distinct colour bands:
    // deep navy, dark blue, medium blue, light blue, pale horizon,
    // green grass, light brown, medium brown, dark brown, rock,
    // deep rock, black void.
    // Plus whatever the default COLOR00 is at frame top.
    // We should see at least 8 distinct colours.
    assert!(
        unique_colors.len() >= 8,
        "Exodus Copper landscape should produce at least 8 distinct colours, found {}",
        unique_colors.len()
    );

    // Save screenshot
    let path = format!("{OUTPUT_DIR}/code198x_exodus_unit01.png");
    {
        let mut png_buf = Vec::new();
        {
            let mut encoder =
                png::Encoder::new(&mut png_buf, viewport.width, viewport.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("PNG header");
            let mut rgba =
                Vec::with_capacity((viewport.width * viewport.height * 4) as usize);
            for &pixel in &viewport.pixels {
                rgba.push(((pixel >> 16) & 0xFF) as u8);
                rgba.push(((pixel >> 8) & 0xFF) as u8);
                rgba.push((pixel & 0xFF) as u8);
                rgba.push(0xFF);
            }
            writer.write_image_data(&rgba).expect("PNG data");
        }
        std::fs::write(&path, &png_buf).expect("write PNG");
    }
    eprintln!("Saved {path}");
}
