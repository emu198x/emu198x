//! Headless capture: PNG screenshots.

use std::error::Error;
use std::fs;
use std::path::Path;

use crate::Atari2600;

/// Save the current framebuffer as a PNG file.
///
/// The framebuffer is ARGB32 (`u32` array). This converts to RGBA bytes
/// for the PNG encoder.
///
/// # Errors
///
/// Returns an error if the file cannot be created or written.
pub fn save_screenshot(system: &Atari2600, path: &Path) -> Result<(), Box<dyn Error>> {
    let width = system.framebuffer_width();
    let height = system.framebuffer_height();
    let fb = system.framebuffer();

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
