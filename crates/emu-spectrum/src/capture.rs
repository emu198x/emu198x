//! Headless capture: PNG screenshots and WAV audio dumps.

#![allow(clippy::cast_possible_truncation)]

use std::error::Error;
use std::fs;
use std::path::Path;

use crate::Spectrum;

/// Save the current framebuffer as a PNG file.
///
/// The framebuffer is ARGB32 (`u32` array). This converts to RGBA bytes
/// for the PNG encoder.
pub fn save_screenshot(spectrum: &Spectrum, path: &Path) -> Result<(), Box<dyn Error>> {
    let width = spectrum.framebuffer_width();
    let height = spectrum.framebuffer_height();
    let fb = spectrum.framebuffer();

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
        rgba.push(0xFF); // Alpha
    }

    writer.write_image_data(&rgba)?;
    Ok(())
}

/// Save audio samples as a WAV file (mono, 48 kHz, 16-bit PCM).
///
/// Input samples are f32 in the range -1.0 to +1.0.
pub fn save_audio(samples: &[f32], path: &Path) -> Result<(), Box<dyn Error>> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(path, spec)?;
    for &sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        let scaled = (clamped * f32::from(i16::MAX)) as i16;
        writer.write_sample(scaled)?;
    }
    writer.finalize()?;
    Ok(())
}

/// Save a sequence of frames as numbered PNGs in a directory.
///
/// Creates `dir/000001.png`, `dir/000002.png`, etc.
pub fn save_frame_sequence(
    spectrum: &mut Spectrum,
    dir: &Path,
    num_frames: u32,
) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(dir)?;

    for i in 1..=num_frames {
        spectrum.run_frame();
        let filename = dir.join(format!("{i:06}.png"));
        save_screenshot(spectrum, &filename)?;
    }

    Ok(())
}

/// Record video + audio: dump frames as PNGs + combined WAV.
///
/// Prints an ffmpeg command to stdout for combining them.
pub fn record(spectrum: &mut Spectrum, dir: &Path, num_frames: u32) -> Result<(), Box<dyn Error>> {
    let frames_dir = dir.join("frames");
    fs::create_dir_all(&frames_dir)?;

    let mut all_audio = Vec::new();

    for i in 1..=num_frames {
        spectrum.run_frame();
        let filename = frames_dir.join(format!("{i:06}.png"));
        save_screenshot(spectrum, &filename)?;

        let audio = spectrum.take_audio_buffer();
        all_audio.extend_from_slice(&audio);
    }

    let audio_path = dir.join("audio.wav");
    save_audio(&all_audio, &audio_path)?;

    eprintln!("Captured {num_frames} frames to {}", frames_dir.display());
    eprintln!("Audio saved to {}", audio_path.display());
    eprintln!();
    eprintln!("Combine with ffmpeg:");
    eprintln!(
        "  ffmpeg -framerate 50 -i {}/frames/%06d.png -i {}/audio.wav -c:v libx264 -pix_fmt yuv420p {}/output.mp4",
        dir.display(),
        dir.display(),
        dir.display()
    );

    Ok(())
}
