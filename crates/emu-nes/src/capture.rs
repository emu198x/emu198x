//! Headless capture: PNG screenshots.

#![allow(clippy::cast_possible_truncation)]

use std::error::Error;
use std::fs;
use std::path::Path;

use crate::Nes;

/// Save the current framebuffer as a PNG file.
///
/// The framebuffer is ARGB32 (`u32` array). This converts to RGBA bytes
/// for the PNG encoder.
///
/// # Errors
///
/// Returns an error if the file cannot be created or written.
pub fn save_screenshot(nes: &Nes, path: &Path) -> Result<(), Box<dyn Error>> {
    let width = nes.framebuffer_width();
    let height = nes.framebuffer_height();
    let fb = nes.framebuffer();

    let file = fs::File::create(path)?;
    let w = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;

    // Convert ARGB32 → RGBA bytes
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for &pixel in fb {
        let r = ((pixel >> 16) & 0xFF) as u8;
        let g = ((pixel >> 8) & 0xFF) as u8;
        let b = (pixel & 0xFF) as u8;
        rgba.push(r);
        rgba.push(g);
        rgba.push(b);
        rgba.push(0xFF);
    }

    writer.write_image_data(&rgba)?;
    Ok(())
}

/// Record video: dump frames as PNGs.
///
/// # Errors
///
/// Returns an error if frames cannot be saved.
pub fn record(nes: &mut Nes, dir: &Path, num_frames: u32) -> Result<(), Box<dyn Error>> {
    let frames_dir = dir.join("frames");
    fs::create_dir_all(&frames_dir)?;

    for i in 1..=num_frames {
        nes.run_frame();
        let filename = frames_dir.join(format!("{i:06}.png"));
        save_screenshot(nes, &filename)?;
    }

    eprintln!("Captured {num_frames} frames to {}", frames_dir.display());
    Ok(())
}
