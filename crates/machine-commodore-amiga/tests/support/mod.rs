//! Test-support helpers for golden-image comparison.
//!
//! Goldens live in `tests/golden/`, captured from FS-UAE running the same
//! ROM + RAM configuration at the matching frame. The emulator's framebuffer
//! must match pixel-exactly. On mismatch, the comparison helper writes
//! `<name>.actual.png` and `<name>.diff.png` next to the golden so the
//! divergence can be inspected visually.
//!
//! Capture procedure: see `wiki/processes/golden-image-capture.md`.

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use commodore_denise_ocs::ViewportPreset;
use machine_commodore_amiga::Amiga;

/// Display dimensions for the PAL Standard viewport, line-doubled and
/// width-halved by `to_display()`. Matches the runtime's screenshot path.
pub const DISPLAY_W: u32 = 768;
pub const DISPLAY_H: u32 = 576;

/// FS-UAE default PAL output dimensions — slightly tighter than full
/// PAL Standard (8 px trimmed each side horizontally, 2 px each side
/// vertically). For golden comparison we crop our 768×576 frame to
/// match this 4:3-equivalent region.
pub const FSUAE_W: u32 = 752;
pub const FSUAE_H: u32 = 572;

/// Render the current Amiga state through the same pipeline the runtime
/// uses for screenshots: PAL Standard viewport, deinterlaced, then
/// `to_display()` (halve width, double height) → 768×576 ARGB8888.
pub fn render_display_frame(amiga: &Amiga) -> (Vec<u32>, u32, u32) {
    let viewport = amiga
        .denise
        .extract_viewport(ViewportPreset::Standard, true, true)
        .to_display();
    (viewport.pixels, viewport.width, viewport.height)
}

/// Crop a 768×576 frame to the 752×572 region FS-UAE captures by
/// default. Symmetric crop: 8 px each side horizontally, 2 px each
/// side vertically. Same 4:3 ratio.
pub fn crop_to_fsuae(pixels: &[u32], src_w: u32, src_h: u32) -> (Vec<u32>, u32, u32) {
    assert_eq!(src_w, DISPLAY_W, "crop expects {DISPLAY_W} px wide source");
    assert_eq!(src_h, DISPLAY_H, "crop expects {DISPLAY_H} px tall source");
    let x_off = (DISPLAY_W - FSUAE_W) / 2; // 8
    let y_off = (DISPLAY_H - FSUAE_H) / 2; // 2
    let mut out = Vec::with_capacity((FSUAE_W * FSUAE_H) as usize);
    for y in 0..FSUAE_H {
        let row = y_off + y;
        let row_start = (row * src_w + x_off) as usize;
        out.extend_from_slice(&pixels[row_start..row_start + FSUAE_W as usize]);
    }
    (out, FSUAE_W, FSUAE_H)
}

/// Render and crop in one step — what golden tests use to match FS-UAE
/// captures.
pub fn render_for_golden(amiga: &Amiga) -> (Vec<u32>, u32, u32) {
    let (pixels, w, h) = render_display_frame(amiga);
    crop_to_fsuae(&pixels, w, h)
}

/// Result of a pixel-exact comparison.
pub struct GoldenComparison {
    pub matches: bool,
    pub differing_pixels: usize,
    pub max_channel_delta: u32,
    pub first_diff: Option<FirstDiff>,
}

pub struct FirstDiff {
    pub x: u32,
    pub y: u32,
    pub actual: u32,
    pub expected: u32,
}

/// Compare the rendered framebuffer to a golden PNG. On mismatch, writes
/// `<golden_path>.actual.png` and `<golden_path>.diff.png` for inspection.
///
/// The golden is read as RGBA8 and its pixel format is normalised to ARGB
/// before comparison so it matches `render_display_frame`'s output.
///
/// Panics if the golden is missing — goldens MUST be present once captured;
/// silent skipping would let regressions slip in.
pub fn assert_matches_golden(
    actual_pixels: &[u32],
    actual_w: u32,
    actual_h: u32,
    golden_path: &Path,
) -> GoldenComparison {
    let (golden_pixels, golden_w, golden_h) = read_png_argb(golden_path)
        .unwrap_or_else(|err| panic!("read golden {}: {err}", golden_path.display()));

    if actual_w != golden_w || actual_h != golden_h {
        write_png_argb(
            &actual_pixels_path(golden_path),
            actual_pixels,
            actual_w,
            actual_h,
        )
        .ok();
        panic!(
            "dimensions differ: actual {actual_w}×{actual_h}, golden {golden_w}×{golden_h} \
             at {}\n\
             actual frame written to {}",
            golden_path.display(),
            actual_pixels_path(golden_path).display(),
        );
    }

    let mut differing = 0usize;
    let mut max_delta = 0u32;
    let mut first_diff: Option<FirstDiff> = None;

    for (idx, (&a, &g)) in actual_pixels.iter().zip(golden_pixels.iter()).enumerate() {
        if a == g {
            continue;
        }
        differing += 1;
        let delta = channel_delta(a, g);
        if delta > max_delta {
            max_delta = delta;
        }
        if first_diff.is_none() {
            first_diff = Some(FirstDiff {
                x: (idx as u32) % actual_w,
                y: (idx as u32) / actual_w,
                actual: a,
                expected: g,
            });
        }
    }

    let comparison = GoldenComparison {
        matches: differing == 0,
        differing_pixels: differing,
        max_channel_delta: max_delta,
        first_diff,
    };

    if !comparison.matches {
        write_png_argb(
            &actual_pixels_path(golden_path),
            actual_pixels,
            actual_w,
            actual_h,
        )
        .ok();
        write_diff_png(
            &diff_path(golden_path),
            actual_pixels,
            &golden_pixels,
            actual_w,
            actual_h,
        )
        .ok();
    }

    comparison
}

fn channel_delta(a: u32, g: u32) -> u32 {
    let ar = (a >> 16) & 0xFF;
    let ag = (a >> 8) & 0xFF;
    let ab = a & 0xFF;
    let gr = (g >> 16) & 0xFF;
    let gg = (g >> 8) & 0xFF;
    let gb = g & 0xFF;
    ar.abs_diff(gr).max(ag.abs_diff(gg)).max(ab.abs_diff(gb))
}

fn actual_pixels_path(golden: &Path) -> PathBuf {
    let mut p = golden.to_path_buf();
    let stem = p.file_stem().unwrap().to_owned();
    let mut new_name = stem;
    new_name.push(".actual.png");
    p.set_file_name(new_name);
    p
}

fn diff_path(golden: &Path) -> PathBuf {
    let mut p = golden.to_path_buf();
    let stem = p.file_stem().unwrap().to_owned();
    let mut new_name = stem;
    new_name.push(".diff.png");
    p.set_file_name(new_name);
    p
}

fn read_png_argb(path: &Path) -> Result<(Vec<u32>, u32, u32), String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let decoder = png::Decoder::new(file);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;

    let bpp = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        other => return Err(format!("unsupported PNG colour type: {other:?}")),
    };

    let pixel_count = (info.width * info.height) as usize;
    let mut pixels = Vec::with_capacity(pixel_count);
    for px in buf.chunks_exact(bpp).take(pixel_count) {
        let r = u32::from(px[0]);
        let g = u32::from(px[1]);
        let b = u32::from(px[2]);
        let a = if bpp == 4 { u32::from(px[3]) } else { 0xFF };
        pixels.push((a << 24) | (r << 16) | (g << 8) | b);
    }
    Ok((pixels, info.width, info.height))
}

fn write_png_argb(path: &Path, pixels: &[u32], w: u32, h: u32) -> Result<(), String> {
    let file = File::create(path).map_err(|e| e.to_string())?;
    let writer = BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    let mut bytes = Vec::with_capacity(pixels.len() * 4);
    for &px in pixels {
        bytes.push(((px >> 16) & 0xFF) as u8);
        bytes.push(((px >> 8) & 0xFF) as u8);
        bytes.push((px & 0xFF) as u8);
        bytes.push(((px >> 24) & 0xFF) as u8);
    }
    writer.write_image_data(&bytes).map_err(|e| e.to_string())?;
    Ok(())
}

/// Render a diff PNG: matching pixels desaturated, mismatching pixels in
/// solid magenta so divergence is visible at a glance.
fn write_diff_png(
    path: &Path,
    actual: &[u32],
    golden: &[u32],
    w: u32,
    h: u32,
) -> Result<(), String> {
    let mut diff = Vec::with_capacity(actual.len());
    for (&a, &g) in actual.iter().zip(golden.iter()) {
        if a == g {
            let r = ((a >> 16) & 0xFF) as u8;
            let gg = ((a >> 8) & 0xFF) as u8;
            let b = (a & 0xFF) as u8;
            let luma = ((u32::from(r) * 299 + u32::from(gg) * 587 + u32::from(b) * 114) / 3000)
                as u8;
            let dim = u32::from(luma);
            diff.push(0xFF00_0000 | (dim << 16) | (dim << 8) | dim);
        } else {
            diff.push(0xFFFF_00FFu32);
        }
    }
    write_png_argb(path, &diff, w, h)
}
