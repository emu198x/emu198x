//! Headless capture: PNG screenshots.

#![allow(clippy::cast_possible_truncation)]

use std::error::Error;
use std::fs;
use std::path::Path;

use crate::Amiga;

/// Save the current framebuffer as a PNG file.
///
/// # Errors
///
/// Returns an error if the file cannot be created or written.
pub fn save_screenshot(amiga: &Amiga, path: &Path) -> Result<(), Box<dyn Error>> {
    let width = amiga.framebuffer_width();
    let height = amiga.framebuffer_height();
    let fb = amiga.framebuffer();

    let file = fs::File::create(path)?;
    let w = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;

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
