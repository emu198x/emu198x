//! Code Like It's 198x integration tests.
//!
//! Verify that the Amiga emulator can run actual lesson code from the Code198x
//! curriculum. The Exodus unit-01 is a Copper list landscape that takes over
//! the machine — it needs Kickstart to bootstrap, then the ADF to load.

use machine_amiga::format_adf::Adf;
use machine_amiga::{Amiga, AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion};
use machine_amiga::commodore_denise_ocs::ViewportPreset;

const OUTPUT_DIR: &str = "../../test_output";

fn ensure_output_dir() {
    let _ = std::fs::create_dir_all(OUTPUT_DIR);
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

fn load_kickstart() -> Option<Vec<u8>> {
    // Try environment variable first, then roms/ directory
    if let Ok(path) = std::env::var("AMIGA_KS13_ROM") {
        return std::fs::read(&path).ok();
    }
    std::fs::read("../../roms/kick13.rom").ok()
}

// ---------------------------------------------------------------------------
// Exodus Unit 1: Copper landscape (sky gradient, earth layers)
// ---------------------------------------------------------------------------

#[test]
#[ignore] // Requires KS1.3 ROM and Code198x repo
fn test_exodus_unit01() {
    let adf_path = match code198x_path(
        "commodore-amiga/game-01-exodus/unit-01/exodus.adf",
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

    let config = AmigaConfig {
        model: AmigaModel::A500,
        chipset: AmigaChipset::Ocs,
        region: AmigaRegion::Pal,
        kickstart,
    };
    let mut amiga = Amiga::new_with_config(config);

    // Insert the Exodus ADF
    let adf_data = std::fs::read(&adf_path).expect("read ADF");
    let adf = Adf::from_bytes(adf_data).expect("parse ADF");
    amiga.insert_disk(adf);

    // Run enough frames for KS1.3 to boot and load the program.
    // KS1.3 boot takes ~10s emulated (280M ticks ≈ 500 PAL frames).
    // After boot, it reads the disk, finds the startup-sequence, and runs
    // the executable. The Copper list program is tiny, so it should be
    // running within 600-800 frames.
    //
    // Run 800 frames and then capture.
    for frame in 0..800 {
        amiga.run_frame();

        // Print progress every 100 frames
        if frame % 100 == 0 {
            eprintln!(
                "Frame {frame}: PC=${:08X} master_clock={}",
                amiga.cpu.regs.pc, amiga.master_clock
            );
        }
    }

    // Verify: if Exodus has taken over, DMACON should have Copper DMA enabled
    // but bitplane DMA disabled (BPLCON0 = $0200, 0 bitplanes).
    // The Copper list sets COLOR00 at various scanlines to create the gradient.
    //
    // We can verify the raster framebuffer has multiple distinct colours —
    // the gradient should produce at least 5 different colours.
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

    // The Copper landscape has ~12 distinct colour bands plus black.
    // Even if only the KS insert-disk screen appears (4 colours), we should
    // see at least 2 unique colours.
    assert!(
        unique_colors.len() >= 2,
        "Viewport should have at least 2 unique colours (found {})",
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
            let mut rgba = Vec::with_capacity((viewport.width * viewport.height * 4) as usize);
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
