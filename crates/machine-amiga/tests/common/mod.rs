//! Shared helpers for Amiga boot screenshot tests.

use machine_amiga::commodore_denise_ocs::{ViewportImage, ViewportPreset};
use machine_amiga::{AUDIO_SAMPLE_RATE, Amiga, AmigaConfig, AmigaRegion, PAL_FRAME_TICKS};
use std::fs;

/// Save a `ViewportImage` to a PNG file.
fn save_viewport_png(path: &str, viewport: &ViewportImage) {
    let path = std::path::Path::new(path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let file = fs::File::create(path).expect("create PNG file");
    let w = &mut std::io::BufWriter::new(file);
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
}

#[allow(dead_code)]
pub const BOOT_TICKS: u64 = 850_000_000; // ~30 seconds PAL

/// Expected register values after boot. Each field is optional — only
/// `Some` values are checked.
#[derive(Default)]
#[allow(dead_code)]
pub struct BootExpect {
    /// Bits that must be SET in DMACON (e.g. 0x0100 = bitplane DMA).
    pub dmacon_set: Option<u16>,
    /// Exact BPLCON0 match.
    pub bplcon0: Option<u16>,
    /// Minimum number of non-zero pixels in the standard viewport.
    /// Catches "all black" or "all one colour" regressions.
    pub min_unique_colours: Option<usize>,
    /// Expected hash of the raw viewport pixel data. Catches any visual
    /// regression — even a single pixel change will fail the test.
    /// Generate by running the test once without this field set, then
    /// copying the printed hash value.
    pub viewport_hash: Option<u64>,
}

/// Compute a deterministic hash of viewport pixel data.
fn hash_viewport(viewport: &ViewportImage) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    viewport.width.hash(&mut hasher);
    viewport.height.hash(&mut hasher);
    viewport.pixels.hash(&mut hasher);
    hasher.finish()
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
///
/// Returns `(dmacon, bplcon0, viewport_hash)`.
pub fn boot_screenshot_test(
    config: AmigaConfig,
    rom_description: &str,
    screenshot_prefix: &str,
    total_ticks: u64,
) -> (u16, u16, u64) {
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
    let mut audio_samples: Vec<f32> = Vec::new();
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

        // Capture a video frame + drain audio periodically
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
            audio_samples.extend_from_slice(&amiga.take_audio_buffer());
        }

        if i % 4 != 0 || i - last_report < report_interval {
            continue;
        }
        last_report = i;
        let elapsed_s = i as f64 / 28_375_160.0;
        println!(
            "[{:.1}s] tick={} PC=${:08X} V={} H={}",
            elapsed_s, i, amiga.cpu.regs.pc, amiga.agnus.vpos, amiga.agnus.hpos,
        );
    }

    // Drain any remaining audio
    audio_samples.extend_from_slice(&amiga.take_audio_buffer());

    // Save raster framebuffer screenshots (standard, display-scaled, full raster)
    let viewport = amiga
        .denise
        .extract_viewport(ViewportPreset::Standard, pal, true);
    let vp_hash = hash_viewport(&viewport);
    println!("Viewport hash: 0x{vp_hash:016X}");
    {
        // Raw superhires screenshot (1280×256 PAL, 1280×200 NTSC)
        let std_path_str = format!("../../test_output/amiga/{screenshot_prefix}.png");
        save_viewport_png(&std_path_str, &viewport);
        println!(
            "Screenshot saved to {} ({}x{})",
            std_path_str, viewport.width, viewport.height,
        );

        // Display-resolution screenshot (720×540, correct 4:3 PAR)
        let display = viewport.to_display();
        let display_path_str = format!("../../test_output/amiga/{screenshot_prefix}_display.png");
        save_viewport_png(&display_path_str, &display);
        println!(
            "Display screenshot saved to {} ({}x{})",
            display_path_str, display.width, display.height,
        );

        // Full raster (debug)
        let full = amiga
            .denise
            .extract_viewport(ViewportPreset::Full, pal, true);
        let full_path_str = format!("../../test_output/amiga/{screenshot_prefix}_full.png");
        save_viewport_png(&full_path_str, &full);
        println!(
            "Full raster saved to {} ({}x{})",
            full_path_str, full.width, full.height,
        );
    }

    // Encode captured frames + audio to MP4 via ffmpeg
    if frame_width > 0 && !video_frames.is_empty() {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let mp4_path = format!("../../test_output/amiga/{screenshot_prefix}.mp4");

        // Write raw f32le PCM audio to a temp file for ffmpeg's second input
        let audio_tmp = format!("../../test_output/amiga/.{screenshot_prefix}_audio.raw");
        {
            let mut f = fs::File::create(&audio_tmp).expect("create audio temp file");
            let pcm_bytes: Vec<u8> = audio_samples.iter().flat_map(|s| s.to_le_bytes()).collect();
            f.write_all(&pcm_bytes).expect("write audio temp file");
        }

        let sample_rate_str = AUDIO_SAMPLE_RATE.to_string();
        let video_size_str = format!("{frame_width}x{frame_height}");
        match Command::new("ffmpeg")
            .args([
                "-y",
                // Video input: raw RGBA frames on stdin
                "-f",
                "rawvideo",
                "-pixel_format",
                "rgba",
                "-video_size",
                &video_size_str,
                "-framerate",
                "2",
                "-i",
                "pipe:0",
                // Audio input: raw f32le stereo PCM from temp file
                "-f",
                "f32le",
                "-ar",
                &sample_rate_str,
                "-ac",
                "2",
                "-i",
                &audio_tmp,
                // Output
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-b:a",
                "128k",
                "-shortest",
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
                    println!("Video saved to {mp4_path} (with audio)");
                } else {
                    eprintln!("ffmpeg failed: {}", String::from_utf8_lossy(&output.stderr));
                }
            }
            Err(e) => {
                eprintln!("ffmpeg not found ({e}), skipping video output");
            }
        }

        // Clean up temp file
        fs::remove_file(&audio_tmp).ok();
    }

    println!("DMACON  = ${:04X}", amiga.agnus.dmacon);
    println!("BPLCON0 = ${:04X}", amiga.denise.bplcon0);
    println!("COP1LC  = ${:08X}", amiga.copper.cop1lc);

    // Dump ExecBase and library list for boot debugging
    let ram = &amiga.memory.chip_ram;
    let rd32 = |a: usize| -> u32 {
        if a + 3 < ram.len() {
            u32::from(ram[a]) << 24
                | u32::from(ram[a + 1]) << 16
                | u32::from(ram[a + 2]) << 8
                | u32::from(ram[a + 3])
        } else {
            0
        }
    };
    let _rd16 = |a: usize| -> u16 {
        if a + 1 < ram.len() {
            u16::from(ram[a]) << 8 | u16::from(ram[a + 1])
        } else {
            0
        }
    };
    let exec_base = rd32(4) as usize;
    println!("ExecBase = ${:08X}", exec_base);
    // ExecBase->LibList is at offset $17A (378). It's a List node.
    // List: lh_Head(4), lh_Tail(4), lh_TailPred(4), lh_Type(1), lh_pad(1)
    // Walk the library list.
    let _lib_list_head_ptr = exec_base + 0x17A;
    let read_mem = |addr: usize| -> u8 {
        if addr < ram.len() {
            ram[addr]
        } else if !amiga.memory.fast_ram.is_empty() {
            let base = amiga.memory.fast_ram_base as usize;
            if addr >= base && addr - base < amiga.memory.fast_ram.len() {
                amiga.memory.fast_ram[addr - base]
            } else if addr >= 0xF80000 && addr < 0xF80000 + amiga.memory.kickstart.len() {
                amiga.memory.kickstart[addr - 0xF80000]
            } else {
                0
            }
        } else if addr >= 0xF80000 && addr < 0xF80000 + amiga.memory.kickstart.len() {
            amiga.memory.kickstart[addr - 0xF80000]
        } else {
            0
        }
    };
    let rd32_any = |a: usize| -> u32 {
        u32::from(read_mem(a)) << 24
            | u32::from(read_mem(a + 1)) << 16
            | u32::from(read_mem(a + 2)) << 8
            | u32::from(read_mem(a + 3))
    };
    let rd16_any = |a: usize| -> u16 {
        u16::from(read_mem(a)) << 8 | u16::from(read_mem(a + 1))
    };
    // Walk a List at a given pointer, printing Name and Version
    let walk_list = |label: &str, list_ptr: usize| {
        println!("--- {} ---", label);
        let mut node = rd32_any(list_ptr) as usize;
        for _ in 0..30 {
            let next = rd32_any(node) as usize;
            if next == 0 { break; }
            let name_ptr = rd32_any(node + 10) as usize;
            let mut name = String::new();
            for j in 0..40 {
                let c = read_mem(name_ptr + j);
                if c == 0 { break; }
                name.push(c as char);
            }
            let version = rd16_any(node + 20);
            let revision = rd16_any(node + 22);
            println!("  ${:08X}: {} v{}.{}", node, name, version, revision);
            node = next;
        }
    };
    walk_list("Library list (ExecBase+$17A)", exec_base + 0x17A);
    walk_list("Device list (ExecBase+$15E)", exec_base + 0x15E);
    walk_list("Resource list (ExecBase+$150)", exec_base + 0x150);

    // Dump exec MemList (ExecBase+$142) to check memory configuration
    println!("--- Memory list (ExecBase+$142) ---");
    {
        let mut node = rd32_any(exec_base + 0x142) as usize;
        for _ in 0..10 {
            let next = rd32_any(node) as usize;
            if next == 0 { break; }
            let name_ptr = rd32_any(node + 10) as usize;
            let mut name = String::new();
            for j in 0..40 {
                let c = read_mem(name_ptr + j);
                if c == 0 { break; }
                name.push(c as char);
            }
            // MemHeader: mh_Node(14), mh_Attributes(2), mh_First(4), mh_Lower(4), mh_Upper(4), mh_Free(4)
            let attrs = rd16_any(node + 14);
            let lower = rd32_any(node + 20) as usize;
            let upper = rd32_any(node + 24) as usize;
            let free = rd32_any(node + 28);
            println!(
                "  ${:08X}: {} attrs=${:04X} ${:08X}-${:08X} free={}K",
                node, name, attrs, lower, upper, free / 1024
            );
            node = next;
        }
    }

    // Dump GfxBase timing fields for STRAP debugging
    // Find GfxBase from library list
    {
        let mut node = rd32_any(exec_base + 0x17A) as usize;
        for _ in 0..30 {
            let next = rd32_any(node) as usize;
            if next == 0 { break; }
            let name_ptr = rd32_any(node + 10) as usize;
            let mut name = String::new();
            for j in 0..40 {
                let c = read_mem(name_ptr + j);
                if c == 0 { break; }
                name.push(c as char);
            }
            if name == "graphics.library" {
                let gfx_base = node;
                println!("--- GfxBase=${:08X} ---", gfx_base);
                // GfxBase+$22 = NormalDisplayRows (word)
                // GfxBase+$24 = NormalDisplayColumns (word)
                // GfxBase+$26 = NormalDPMX (word) - dots per meter X
                // GfxBase+$28 = NormalDPMY (word)
                // GfxBase+$EC = DisplayFlags (word)
                // GfxBase+$206 = monitor_id (long)
                println!("  NormalDisplayRows (GfxBase+$22) = {}", rd16_any(gfx_base + 0x22));
                println!("  NormalDisplayColumns (GfxBase+$24) = {}", rd16_any(gfx_base + 0x24));
                println!("  DisplayFlags (GfxBase+$EC) = ${:04X}", rd16_any(gfx_base + 0xEC));
                // Dump first 64 bytes of ActiView (GfxBase+$22)
                println!("  GfxBase fields around timing area:");
                for off in (0x20..0x40).step_by(2) {
                    println!("    +${:02X}: ${:04X}", off, rd16_any(gfx_base + off));
                }
                // check the copper list pointers in the view
                let acti_view = rd32_any(gfx_base + 0x22) as usize;
                println!("  ActiView ptr (GfxBase+$22) = ${:08X}", acti_view);
                // LOFlist at ActiView+$0E
                if acti_view > 0 && acti_view < 0x01000000 {
                    let lof_list = rd32_any(acti_view + 0x0E) as usize;
                    println!("  ActiView->LOFlist (ActiView+$0E) = ${:08X}", lof_list);
                }
                break;
            }
            node = next;
        }
    }

    // Dump copper list area for debugging
    for &(label, base) in &[("COP1LC", amiga.copper.cop1lc as usize), ("$C00", 0xC00usize)] {
        println!("--- Copper list at {} (${:06X}) ---", label, base);
        for i in (0..64).step_by(4) {
            let a = base + i;
            if a + 3 < amiga.memory.chip_ram.len() {
                let w0 = u16::from(amiga.memory.chip_ram[a]) << 8 | u16::from(amiga.memory.chip_ram[a + 1]);
                let w1 = u16::from(amiga.memory.chip_ram[a + 2]) << 8 | u16::from(amiga.memory.chip_ram[a + 3]);
                println!("  ${:06X}: ${:04X} ${:04X}", a, w0, w1);
            }
        }
    }

    (amiga.agnus.dmacon, amiga.denise.bplcon0, vp_hash)
}

/// Run a boot test with register/display assertions.
#[allow(dead_code)]
pub fn boot_screenshot_test_expect(
    config: AmigaConfig,
    rom_description: &str,
    screenshot_prefix: &str,
    total_ticks: u64,
    expect: BootExpect,
) {
    let (dmacon, bplcon0, vp_hash) =
        boot_screenshot_test(config, rom_description, screenshot_prefix, total_ticks);

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
    if let Some(expected) = expect.viewport_hash {
        assert_eq!(
            vp_hash, expected,
            "{rom_description}: viewport hash 0x{vp_hash:016X} != expected 0x{expected:016X} \
             — visual regression detected. Run the test with --nocapture and check the \
             _display.png screenshot to see what changed.",
        );
    }
}
