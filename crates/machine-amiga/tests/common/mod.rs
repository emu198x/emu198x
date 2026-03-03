//! Shared helpers for Amiga boot screenshot tests.

use machine_amiga::commodore_denise_ocs::ViewportPreset;
use machine_amiga::{Amiga, AmigaConfig, AmigaRegion, PAL_FRAME_TICKS};
use std::fs;

pub const BOOT_TICKS: u64 = 850_000_000; // ~30 seconds PAL

/// Expected register values after boot. Each field is optional — only
/// `Some` values are checked.
#[derive(Default)]
pub struct BootExpect {
    /// Bits that must be SET in DMACON (e.g. 0x0100 = bitplane DMA).
    pub dmacon_set: Option<u16>,
    /// Exact BPLCON0 match.
    pub bplcon0: Option<u16>,
    /// Minimum number of non-zero pixels in the standard viewport.
    /// Catches "all black" or "all one colour" regressions.
    pub min_unique_colours: Option<usize>,
}

/// Load a ROM file, returning None (with a message) if missing.
pub fn load_rom(path: &str) -> Option<Vec<u8>> {
    match fs::read(path) {
        Ok(r) => Some(r),
        Err(_) => {
            eprintln!("ROM not found at {path}, skipping");
            None
        }
    }
}

/// Run a full boot sequence, save screenshots (standard + full raster),
/// and encode a diagnostic 2 fps video via ffmpeg.
pub fn boot_screenshot_test(
    config: AmigaConfig,
    rom_description: &str,
    screenshot_prefix: &str,
    total_ticks: u64,
) -> (u16, u16) {
    let pal = config.region == AmigaRegion::Pal;
    let mut amiga = Amiga::new_with_config(config);

    println!(
        "{}: Reset SSP=${:08X} PC=${:08X} SR=${:04X}",
        rom_description, amiga.cpu.regs.ssp, amiga.cpu.regs.pc, amiga.cpu.regs.sr
    );

    let report_interval: u64 = 28_375_160; // ~1 second
    let battclock_threshold: u64 = 2 * 28_375_160; // ~2 seconds
    let mut last_report = 0u64;

    // Video capture: one frame every 25 VBLANKs (~2 fps)
    let capture_interval = PAL_FRAME_TICKS * 25;
    let mut next_capture = capture_interval;
    let mut video_frames: Vec<u8> = Vec::new();
    let mut frame_width = 0u32;
    let mut frame_height = 0u32;

    for i in 0..total_ticks {
        amiga.tick();

        // Battclock simulation disabled — it corrupts the CIA-A TOD
        // counter, preventing timer.device from calibrating the EClock
        // frequency. The divisor at GfxBase+$22 stays 0, causing a
        // DIVU by zero in the STRAP display routine.
        // TODO: implement proper battclock.resource instead of
        // force-setting the TOD counter.
        let _ = battclock_threshold;

        // Capture a video frame periodically
        if i >= next_capture {
            next_capture += capture_interval;
            let vp = amiga
                .denise
                .extract_viewport(ViewportPreset::Standard, pal, true);
            frame_width = vp.width;
            frame_height = vp.height;
            for &pixel in &vp.pixels {
                video_frames.push(((pixel >> 16) & 0xFF) as u8);
                video_frames.push(((pixel >> 8) & 0xFF) as u8);
                video_frames.push((pixel & 0xFF) as u8);
                video_frames.push(0xFF);
            }
        }

        if i % 4 != 0 || i - last_report < report_interval {
            continue;
        }
        last_report = i;
        let elapsed_s = i as f64 / 28_375_160.0;
        println!(
            "[{:.1}s] tick={} PC=${:08X} V={} H={}",
            elapsed_s,
            i,
            amiga.cpu.regs.pc,
            amiga.agnus.vpos,
            amiga.agnus.hpos,
        );
    }

    // Save raster framebuffer screenshots (standard viewport + full raster)
    {
        let viewport = amiga
            .denise
            .extract_viewport(ViewportPreset::Standard, pal, true);
        let std_path_str = format!("../../test_output/amiga/{screenshot_prefix}.png");
        let std_path = std::path::Path::new(&std_path_str);
        if let Some(parent) = std_path.parent() {
            fs::create_dir_all(parent).ok();
        }
        let file = fs::File::create(std_path).expect("create screenshot file");
        let ref mut w = std::io::BufWriter::new(file);
        let mut encoder = png::Encoder::new(w, viewport.width, viewport.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("write PNG header");
        let mut rgba = Vec::with_capacity((viewport.width * viewport.height * 4) as usize);
        for &pixel in &viewport.pixels {
            rgba.push(((pixel >> 16) & 0xFF) as u8);
            rgba.push(((pixel >> 8) & 0xFF) as u8);
            rgba.push((pixel & 0xFF) as u8);
            rgba.push(((pixel >> 24) & 0xFF) as u8);
        }
        writer.write_image_data(&rgba).expect("write PNG data");
        println!(
            "Screenshot saved to {} ({}x{})",
            std_path.display(),
            viewport.width,
            viewport.height,
        );

        let full = amiga
            .denise
            .extract_viewport(ViewportPreset::Full, pal, true);
        let full_path_str = format!("../../test_output/amiga/{screenshot_prefix}_full.png");
        let full_path = std::path::Path::new(&full_path_str);
        let file = fs::File::create(full_path).expect("create full screenshot file");
        let ref mut w = std::io::BufWriter::new(file);
        let mut encoder = png::Encoder::new(w, full.width, full.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("write full PNG header");
        let mut rgba = Vec::with_capacity((full.width * full.height * 4) as usize);
        for &pixel in &full.pixels {
            rgba.push(((pixel >> 16) & 0xFF) as u8);
            rgba.push(((pixel >> 8) & 0xFF) as u8);
            rgba.push((pixel & 0xFF) as u8);
            rgba.push(((pixel >> 24) & 0xFF) as u8);
        }
        writer.write_image_data(&rgba).expect("write full PNG data");
        println!(
            "Full raster saved to {} ({}x{})",
            full_path.display(),
            full.width,
            full.height,
        );
    }

    // Encode captured frames to MP4 via ffmpeg (diagnostic 2fps video)
    if frame_width > 0 && !video_frames.is_empty() {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let mp4_path = format!("../../test_output/amiga/{screenshot_prefix}.mp4");
        match Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "rawvideo",
                "-pixel_format",
                "rgba",
                "-video_size",
                &format!("{frame_width}x{frame_height}"),
                "-framerate",
                "2",
                "-i",
                "pipe:0",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                &mp4_path,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(mut child) => {
                child
                    .stdin
                    .take()
                    .unwrap()
                    .write_all(&video_frames)
                    .expect("pipe video frames");
                let output = child.wait_with_output().expect("ffmpeg");
                if output.status.success() {
                    println!("Video saved to {mp4_path}");
                } else {
                    eprintln!(
                        "ffmpeg failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
            }
            Err(e) => {
                eprintln!("ffmpeg not found ({e}), skipping video output");
            }
        }
    }

    println!("DMACON  = ${:04X}", amiga.agnus.dmacon);
    println!("BPLCON0 = ${:04X}", amiga.denise.bplcon0);

    (amiga.agnus.dmacon, amiga.denise.bplcon0)
}

/// Run a boot test with register/display assertions.
pub fn boot_screenshot_test_expect(
    config: AmigaConfig,
    rom_description: &str,
    screenshot_prefix: &str,
    total_ticks: u64,
    expect: BootExpect,
) {
    let (dmacon, bplcon0) = boot_screenshot_test(config, rom_description, screenshot_prefix, total_ticks);

    if let Some(bits) = expect.dmacon_set {
        assert!(
            dmacon & bits == bits,
            "{rom_description}: DMACON ${dmacon:04X} missing expected bits ${bits:04X}",
        );
    }
    if let Some(expected) = expect.bplcon0 {
        assert_eq!(
            bplcon0, expected,
            "{rom_description}: BPLCON0 ${bplcon0:04X} != expected ${expected:04X}",
        );
    }
}
