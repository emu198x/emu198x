//! VICE VICII testbench — pixel-oracle validation for the VIC-II VC/VCBASE/RC
//! rewrite (Increment 5).
//!
//! The VICE test programs (`~/.emu198x/test-suites/c64-vicii/`, external +
//! env-gated) render patterns whose *correctness is the rendered image*; each
//! ships a reference PNG per chip revision. This harness boots real C64 ROMs,
//! runs a test program, and compares our framebuffer against VICE's PAL 6569
//! reference (`<name>.prg.png`).
//!
//! Gated on both the ROM dir (`~/.emu198x/roms/commodore-c64/`) and the
//! testbench dir; skips cleanly when either is absent. See
//! `docs/plans/2026-06-30-c64-vic-ii-vc-vcbase-rc-rewrite.md` (Increment 5)
//! and the memory note `project_c64_vicii_testbench`.

mod common;

use std::path::PathBuf;

use common::local_rom_firmware;
use common_commodore_c64::timing::{TIMING_NTSC_BREADBIN, TIMING_PAL_BREADBIN};
use emu198x_shell::HeadlessSession;
use mos_vic_ii::{FB_HEIGHT, FB_WIDTH};
use runtime_commodore_c64::{
    C64Runtime, C64SessionQueryProvider, DEFAULT_INTER_CHAR_FRAMES, DEFAULT_KEY_HOLD_FRAMES,
    DEFAULT_TYPE_SETTLE_FRAMES, Model, type_string,
};
use sha2::{Digest, Sha256};

/// The explicitly configured testbench, or the conventional per-user staging
/// directory when no explicit path is supplied.
fn testbench_dir() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("EMU198X_C64_VICII_TESTBENCH_DIR") {
        let path = PathBuf::from(path);
        return path.exists().then_some(path);
    }
    let home = std::env::var("HOME").ok()?;
    let path = PathBuf::from(home).join(".emu198x/test-suites/c64-vicii");
    path.exists().then_some(path)
}

/// True when the real C64 ROM set is staged locally.
fn roms_present() -> bool {
    let rom_dir = std::env::var("EMU198X_C64_ROM_DIR")
        .map(PathBuf::from)
        .or_else(|_| {
            std::env::var("HOME")
                .map(|home| PathBuf::from(home).join(".emu198x/roms/commodore-c64"))
        });
    rom_dir
        .map(|path| {
            ["kernal.rom", "basic.rom", "chargen.rom"]
                .iter()
                .all(|name| path.join(name).is_file())
        })
        .unwrap_or(false)
}

/// Boot real ROMs, load a testbench `.prg` (relative to the testbench dir),
/// RUN it, settle for `settle_frames`, and return the ARGB framebuffer. Sprites
/// render through the draw-stage sequencer (the shipping default).
fn run_testprog(rel_prg: &str, settle_frames: u32) -> Vec<u32> {
    run_testprog_on(
        rel_prg,
        settle_frames,
        Model::C64PalBreadbin,
        TIMING_PAL_BREADBIN.cycles_per_frame,
    )
}

/// As `run_testprog`, but on an explicit model (PAL vs NTSC) + its frame length.
fn run_testprog_on(
    rel_prg: &str,
    settle_frames: u32,
    model: Model,
    cycles_per_frame: u32,
) -> Vec<u32> {
    let dir = testbench_dir().expect("testbench dir checked by caller");
    let prg = std::fs::read(dir.join(rel_prg)).expect("testbench .prg should read");

    let firmware = local_rom_firmware();
    let runtime = C64Runtime::from_firmware(model, &firmware)
        .expect("real C64 firmware should construct a runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(cycles_per_frame),
        C64SessionQueryProvider,
    );

    // Boot to the READY prompt (real hardware ~2.5 s; 150 PAL frames = 3 s).
    session.run_frames(150).expect("boot should run");

    // Load the program image into RAM and fix the BASIC end-of-program pointer
    // (VARTAB, $2D/$2E) so RUN finds the program's end and the SYS stub runs.
    let load_addr = session
        .machine_mut()
        .load_prg_bytes(&prg)
        .expect("testbench .prg should load");
    let end = load_addr + (prg.len() as u16 - 2);
    {
        let machine = session.machine_mut().machine_mut();
        machine.cpu_write(0x2D, (end & 0xFF) as u8);
        machine.cpu_write(0x2E, (end >> 8) as u8);
    }

    type_string(
        &mut session,
        "RUN\n",
        DEFAULT_KEY_HOLD_FRAMES,
        DEFAULT_TYPE_SETTLE_FRAMES,
    )
    .expect("typing RUN should succeed");
    session
        .run_frames(settle_frames)
        .expect("settle frames should run");

    session.machine_mut().machine_mut().framebuffer().to_vec()
}

/// Write an ARGB framebuffer as an RGB PNG (calibration aid).
fn write_framebuffer_png(path: &str, fb: &[u32]) {
    let mut rgb = Vec::with_capacity(fb.len() * 3);
    for &px in fb {
        rgb.push((px >> 16) as u8);
        rgb.push((px >> 8) as u8);
        rgb.push(px as u8);
    }
    let file = std::fs::File::create(path).expect("png path should be writable");
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), FB_WIDTH, FB_HEIGHT);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(&rgb).expect("png data");
}

/// Crop offset of our 416x312 framebuffer to VICE's 384x272 reference window,
/// derived by `calibrate_gfxfetch_alignment`: 16px extra border, left and top.
const VICE_CROP_X: u32 = 16;
const VICE_CROP_Y: u32 = 16;

/// A decoded reference PNG: RGB pixels + dimensions.
struct RefImage {
    width: u32,
    height: u32,
    color_type: &'static str,
    rgb: Vec<u8>,
}

fn decode_reference_png(path: &PathBuf) -> RefImage {
    let file = std::fs::File::open(path).expect("reference PNG should open");
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let mut reader = decoder.read_info().expect("reference PNG header");
    let mut buf = vec![0u8; reader.output_buffer_size().expect("png buffer size")];
    let info = reader.next_frame(&mut buf).expect("reference PNG frame");
    buf.truncate(info.buffer_size());
    // Reference PNGs are 8-bit; normalise to RGB (drop alpha if present).
    let (color_type, rgb) = match info.color_type {
        png::ColorType::Rgb => ("rgb8", buf),
        png::ColorType::Rgba => (
            "rgba8",
            buf.chunks_exact(4)
                .flat_map(|p| [p[0], p[1], p[2]])
                .collect(),
        ),
        other => panic!("unexpected reference PNG colour type: {other:?}"),
    };
    RefImage {
        width: info.width,
        height: info.height,
        color_type,
        rgb,
    }
}

/// Nearest C64 colour index (0-15) for an RGB triple, by squared Euclidean
/// distance to our palette. VICE's reference PNGs use a different palette
/// (different RGB byte values for the same 16 colours), so comparison is by
/// colour *index*, not raw RGB.
fn nearest_c64_index(r: u8, g: u8, b: u8) -> u8 {
    let mut best = (u32::MAX, 0u8);
    for (i, &argb) in mos_vic_ii::palette::PALETTE.iter().enumerate() {
        let (pr, pg, pb) = ((argb >> 16) as u8, (argb >> 8) as u8, argb as u8);
        let d = (i32::from(r) - i32::from(pr)).pow(2)
            + (i32::from(g) - i32::from(pg)).pow(2)
            + (i32::from(b) - i32::from(pb)).pow(2);
        if (d as u32) < best.0 {
            best = (d as u32, i as u8);
        }
    }
    best.1
}

/// Fraction of pixels whose C64 colour index matches when the reference is
/// placed at (`dx`,`dy`) within our framebuffer.
struct IndexedComparison {
    matched_pixels: usize,
    actual: Vec<u8>,
    reference: Vec<u8>,
}

fn compare_indexed(fb: &[u32], reference: &RefImage, dx: u32, dy: u32) -> IndexedComparison {
    let mut matched_pixels = 0usize;
    let total = (reference.width * reference.height) as usize;
    let mut actual_indices = Vec::with_capacity(total);
    let mut reference_indices = Vec::with_capacity(total);
    for ry in 0..reference.height {
        for rx in 0..reference.width {
            let oi = ((dy + ry) * FB_WIDTH + (dx + rx)) as usize;
            let px = fb[oi];
            let ours = nearest_c64_index((px >> 16) as u8, (px >> 8) as u8, px as u8);
            let ri = ((ry * reference.width + rx) * 3) as usize;
            let theirs = nearest_c64_index(
                reference.rgb[ri],
                reference.rgb[ri + 1],
                reference.rgb[ri + 2],
            );
            actual_indices.push(ours);
            reference_indices.push(theirs);
            if ours == theirs {
                matched_pixels += 1;
            }
        }
    }
    IndexedComparison {
        matched_pixels,
        actual: actual_indices,
        reference: reference_indices,
    }
}

fn match_fraction(fb: &[u32], reference: &RefImage, dx: u32, dy: u32) -> f64 {
    let comparison = compare_indexed(fb, reference, dx, dy);
    comparison.matched_pixels as f64 / comparison.reference.len() as f64
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Calibration: dump our `gfxfetch` framebuffer and search for the crop offset
/// that best matches VICE's 384x272 reference, reporting the best alignment +
/// match fraction. Drives the constants baked into the real comparison test.
#[test]
#[ignore = "calibration aid: derives crop offset + palette match vs VICE reference"]
fn calibrate_gfxfetch_alignment() {
    if !roms_present() || testbench_dir().is_none() {
        eprintln!("skip: C64 ROMs or testbench not staged");
        return;
    }
    let dir = testbench_dir().expect("checked");
    let reference = decode_reference_png(&dir.join("gfxfetch/references/gfxfetch.prg.png"));
    eprintln!("reference {}x{}", reference.width, reference.height);

    let fb = run_testprog("gfxfetch/gfxfetch.prg", 60);
    write_framebuffer_png("/tmp/vicii_gfxfetch_ours.png", &fb);

    // Offset (16,16) established by the full search: our 416x312 buffer has
    // 16px extra border on the left and top vs VICE's 384x272 window.
    let (dx, dy) = (16u32, 16u32);
    eprintln!(
        "match at ({dx},{dy}) = {:.4}%",
        match_fraction(&fb, &reference, dx, dy) * 100.0
    );

    // Dump a diff image: matching pixels in our colour, mismatches in magenta.
    let mut diff = Vec::with_capacity((reference.width * reference.height * 3) as usize);
    for ry in 0..reference.height {
        for rx in 0..reference.width {
            let px = fb[((dy + ry) * FB_WIDTH + (dx + rx)) as usize];
            let ours = nearest_c64_index((px >> 16) as u8, (px >> 8) as u8, px as u8);
            let ri = ((ry * reference.width + rx) * 3) as usize;
            let theirs = nearest_c64_index(
                reference.rgb[ri],
                reference.rgb[ri + 1],
                reference.rgb[ri + 2],
            );
            if ours == theirs {
                diff.extend_from_slice(&[(px >> 16) as u8, (px >> 8) as u8, px as u8]);
            } else {
                diff.extend_from_slice(&[0xFF, 0x00, 0xFF]);
            }
        }
    }
    let file = std::fs::File::create("/tmp/vicii_gfxfetch_diff.png").expect("diff path");
    let mut enc = png::Encoder::new(
        std::io::BufWriter::new(file),
        reference.width,
        reference.height,
    );
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .expect("diff header")
        .write_image_data(&diff)
        .expect("diff data");
    eprintln!("wrote /tmp/vicii_gfxfetch_diff.png (mismatches in magenta)");
}

/// Like `match_fraction`, but tolerant of a reference taller/wider than our
/// framebuffer: reference pixels that fall outside our `fb` (given its
/// `fb_width`×`fb_height`) count as mismatches. Lets the NTSC search run even
/// though VICE's 247-line reference is taller than our 244-line NTSC buffer.
fn match_fraction_bounded(
    fb: &[u32],
    fb_width: u32,
    fb_height: u32,
    reference: &RefImage,
    dx: u32,
    dy: u32,
) -> f64 {
    let mut matched = 0usize;
    let total = (reference.width * reference.height) as usize;
    for ry in 0..reference.height {
        for rx in 0..reference.width {
            let (ox, oy) = (dx + rx, dy + ry);
            if ox >= fb_width || oy >= fb_height {
                continue; // out of our window → mismatch
            }
            let px = fb[(oy * fb_width + ox) as usize];
            let ours = nearest_c64_index((px >> 16) as u8, (px >> 8) as u8, px as u8);
            let ri = ((ry * reference.width + rx) * 3) as usize;
            let theirs = nearest_c64_index(
                reference.rgb[ri],
                reference.rgb[ri + 1],
                reference.rgb[ri + 2],
            );
            if ours == theirs {
                matched += 1;
            }
        }
    }
    matched as f64 / total as f64
}

/// Calibration: find the crop offset that best aligns our **NTSC 6567R8**
/// `gfxfetch` output to VICE's 384x247 NTSC reference, and report it. VICE's
/// NTSC window is the same 384px width as PAL but 247 lines tall (vs our 244),
/// so this quantifies both the crop offset and the vertical-extent gap.
#[test]
#[ignore = "calibration aid: NTSC crop offset + match vs VICE 6567R8 reference"]
fn calibrate_ntsc_gfxfetch_alignment() {
    if !roms_present() || testbench_dir().is_none() {
        eprintln!("skip: C64 ROMs or testbench not staged");
        return;
    }
    let dir = testbench_dir().expect("checked");
    let reference = decode_reference_png(&dir.join("gfxfetch/references/gfxfetch_ntsc.prg.png"));
    eprintln!("NTSC reference {}x{}", reference.width, reference.height);

    let fb = run_testprog_on(
        "gfxfetch/gfxfetch_ntsc.prg",
        60,
        Model::C64NtscBreadbin,
        TIMING_NTSC_BREADBIN.cycles_per_frame,
    );
    let fb_height = (fb.len() as u32) / FB_WIDTH;
    eprintln!("our NTSC framebuffer {}x{}", FB_WIDTH, fb_height);

    // Dump our raw NTSC framebuffer for eyeballing vs the reference.
    {
        let mut rgb = Vec::with_capacity(fb.len() * 3);
        for &px in &fb {
            rgb.extend_from_slice(&[(px >> 16) as u8, (px >> 8) as u8, px as u8]);
        }
        let file = std::fs::File::create("/tmp/vicii_ntsc_ours.png").expect("png");
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), FB_WIDTH, fb_height);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .expect("png header")
            .write_image_data(&rgb)
            .expect("png data");
        eprintln!("wrote /tmp/vicii_ntsc_ours.png");
    }

    let max_dy = fb_height.saturating_sub(reference.height) + 24;
    let mut best = (0.0f64, 0u32, 0u32);
    for dy in 0..=max_dy {
        for dx in 0..=(FB_WIDTH - reference.width) {
            let m = match_fraction_bounded(&fb, FB_WIDTH, fb_height, &reference, dx, dy);
            if m > best.0 {
                best = (m, dx, dy);
            }
        }
    }
    eprintln!(
        "best NTSC align dx={} dy={} match={:.3}%",
        best.1,
        best.2,
        best.0 * 100.0
    );
}

/// NTSC 6567R8 `gfxfetch` against VICE's 384x247 reference, at the calibrated
/// crop (dx=16 — identical to PAL — dy=28). Locks NTSC content correctness as a
/// regression floor.
///
/// Matches ~94.4% overall; on the non-wrapped rows that is ~99.3%, on par with
/// the PAL floor (99.33%). The residual to 100% is **not** a rendering error:
/// VICE's NTSC visible window wraps the frame boundary, so its last ~12
/// reference rows are top-of-frame content that we render at the top of the
/// full frame instead of the bottom of a cropped window. We render every raster
/// line (like PAL) and leave cropping to the consumer.
#[test]
#[ignore = "requires ~/.emu198x/roms/commodore-c64 + ~/.emu198x/test-suites/c64-vicii"]
fn ntsc_gfxfetch_matches_vice_reference() {
    if !roms_present() || testbench_dir().is_none() {
        eprintln!("skip: C64 ROMs or testbench not staged");
        return;
    }
    let dir = testbench_dir().expect("checked");
    let reference = decode_reference_png(&dir.join("gfxfetch/references/gfxfetch_ntsc.prg.png"));
    let fb = run_testprog_on(
        "gfxfetch/gfxfetch_ntsc.prg",
        60,
        Model::C64NtscBreadbin,
        TIMING_NTSC_BREADBIN.cycles_per_frame,
    );
    let fb_height = (fb.len() as u32) / FB_WIDTH;
    let m = match_fraction_bounded(&fb, FB_WIDTH, fb_height, &reference, 16, 28);
    assert!(
        m >= 0.94,
        "NTSC gfxfetch regressed vs VICE 6567R8: {:.3}% < 94%",
        m * 100.0
    );
}

/// `gfxfetch` — the in-line graphics-fetch timing test — rendered against
/// VICE's PAL 6569 reference, compared by C64 colour index (VICE's PNG uses a
/// different palette, so raw RGB won't match).
///
/// Our engine currently matches VICE at **99.33%**, stable across settle time.
/// This locks that as a **regression floor** — it catches any change that
/// makes the rewrite render this timing trick *worse*. It is deliberately
/// **not** a 100%/"pass" claim: the residual ~0.7% sits in the in-line-fetch
/// test region (a full-width raster stripe + the VSP test row), a genuine
/// cycle-timing gap toward exactness that is open Increment 5 work. See
/// `/tmp/vicii_gfxfetch_diff.png` from `calibrate_gfxfetch_alignment`.
#[test]
#[ignore = "requires ~/.emu198x/roms/commodore-c64 + ~/.emu198x/test-suites/c64-vicii"]
fn gfxfetch_matches_vice_reference() {
    if !roms_present() || testbench_dir().is_none() {
        eprintln!("skip: C64 ROMs or testbench not staged");
        return;
    }
    let dir = testbench_dir().expect("checked");
    let reference = decode_reference_png(&dir.join("gfxfetch/references/gfxfetch.prg.png"));
    let fb = run_testprog("gfxfetch/gfxfetch.prg", 60);
    let m = match_fraction(&fb, &reference, VICE_CROP_X, VICE_CROP_Y);
    assert!(
        m >= 0.99,
        "gfxfetch vs VICE 6569 dropped below the 99% regression floor: {:.4}%",
        m * 100.0
    );
}

/// Diagnostic: dump one program's framebuffer to `/tmp/vicii_dump.png`. The
/// program path (relative to the testbench dir) comes from `VICII_DUMP_PRG`.
#[test]
#[ignore = "diagnostic: dumps framebuffer for the VICII_DUMP_PRG program"]
fn dump_prg_framebuffer() {
    if !roms_present() || testbench_dir().is_none() {
        eprintln!("skip: C64 ROMs or testbench not staged");
        return;
    }
    let Ok(rel) = std::env::var("VICII_DUMP_PRG") else {
        eprintln!("set VICII_DUMP_PRG=<category/name.prg>");
        return;
    };
    let fb = run_testprog(&rel, 60);
    write_framebuffer_png("/tmp/vicii_dump.png", &fb);
    eprintln!("wrote /tmp/vicii_dump.png for {rel}");
}

/// Rewrite-relevant testbench cases: (label, program, reference PNG). The
/// selection covers 13 PAL 6569 categories and broadens the colour-fetch-bug
/// category to all five programs. All references are 384x272.
const SURVEY: &[(&str, &str, &str)] = &[
    (
        "gfxfetch",
        "gfxfetch/gfxfetch.prg",
        "gfxfetch/references/gfxfetch.prg.png",
    ),
    (
        "dmadelay",
        "dmadelay/test1-2a-03.prg",
        "dmadelay/references/test1-2a-03.prg.png",
    ),
    (
        "colorfetchbug",
        "colorfetchbug/bitmap.prg",
        "colorfetchbug/references/bitmap.prg.png",
    ),
    (
        "colorfetchbug-main",
        "colorfetchbug/main.prg",
        "colorfetchbug/references/main.prg.png",
    ),
    (
        "colorfetchbug-main2",
        "colorfetchbug/main2.prg",
        "colorfetchbug/references/main2.prg.png",
    ),
    (
        "colorfetchbug-main3",
        "colorfetchbug/main3.prg",
        "colorfetchbug/references/main3.prg.png",
    ),
    (
        "colorfetchbug-main4",
        "colorfetchbug/main4.prg",
        "colorfetchbug/references/main4.prg.png",
    ),
    (
        "sequencer-bug",
        "sequencer-bug/bug.prg",
        "sequencer-bug/references/bug.prg.png",
    ),
    (
        "greydot",
        "greydot/greydot.prg",
        "greydot/references/greydot.prg.png",
    ),
    (
        "spritecrunch",
        "spritecrunch/spritecrunch-3b-00.prg",
        "spritecrunch/references/spritecrunch-3b-00.prg.png",
    ),
    (
        "spritedma",
        "spritedma/d017-54.prg",
        "spritedma/references/d017-54.prg.png",
    ),
    (
        "spritefetchbug",
        "spritefetchbug/test-136-2a.prg",
        "spritefetchbug/references/test-136-2a.prg.png",
    ),
    (
        "sb_sprite_fetch",
        "sb_sprite_fetch/sbsprf24-163.prg",
        "sb_sprite_fetch/references/sbsprf24-163.prg.png",
    ),
    (
        "vicii_timing",
        "vicii_timing/vicii_reg_timing-a5.prg",
        "vicii_timing/references/vicii_reg_timing-a5.prg.png",
    ),
    (
        "videomode",
        "videomode/rmwtest.prg",
        "videomode/references/rmwtest.prg.png",
    ),
    (
        "border",
        "border/border-250.prg",
        "border/references/border-250.prg.png",
    ),
    (
        "screenpos",
        "screenpos/screenpos.prg",
        "screenpos/references/screenpos.prg.png",
    ),
];

/// Breadth survey: run each category and report its match % against VICE
/// (worst first). Reveals whether `gfxfetch`'s residual is isolated or a
/// systematic sub-cycle offset. Not a pass/fail — a measurement dashboard.
#[test]
#[ignore = "survey aid: measures match % across VICII testbench categories"]
fn survey_testbench_categories() {
    let result_path = std::env::var("EMU198X_C64_VICII_SURVEY_RESULT")
        .ok()
        .map(PathBuf::from);
    if !roms_present() || testbench_dir().is_none() {
        assert!(
            result_path.is_none(),
            "report-mode VIC-II survey requires the configured C64 ROM set and testbench"
        );
        eprintln!("skip: C64 ROMs or testbench not staged");
        return;
    }
    let dir = testbench_dir().expect("checked");
    let mut rows: Vec<(f64, &str)> = Vec::new();
    let mut cases = Vec::with_capacity(SURVEY.len());
    for (label, prg, refpng) in SURVEY {
        let refpath = dir.join(refpng);
        if !refpath.exists() {
            assert!(
                result_path.is_none(),
                "report-mode VIC-II survey is missing reference {refpng}"
            );
            eprintln!("{label:16} MISSING reference");
            continue;
        }
        let reference = decode_reference_png(&refpath);
        let fb = run_testprog(prg, 60);
        let comparison = compare_indexed(&fb, &reference, VICE_CROP_X, VICE_CROP_Y);
        let total_pixels = comparison.reference.len();
        let fraction = comparison.matched_pixels as f64 / total_pixels as f64;
        rows.push((fraction, label));
        cases.push(serde_json::json!({
            "id": label,
            "program": prg,
            "reference": refpng,
            "reference_width": reference.width,
            "reference_height": reference.height,
            "reference_color_type": reference.color_type,
            "reference_indexed_sha256": sha256_hex(&comparison.reference),
            "actual_indexed_sha256": sha256_hex(&comparison.actual),
            "matched_pixels": comparison.matched_pixels,
            "total_pixels": total_pixels,
        }));
    }
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    eprintln!("\n=== VICII survey vs VICE 6569 (match %, worst first) ===");
    for (m, label) in &rows {
        eprintln!("{:7.3}%  {label}", m * 100.0);
    }

    if let Some(result_path) = result_path {
        let revision = std::env::var("EMU198X_ACCURACY_GIT_REVISION")
            .expect("report-mode survey requires EMU198X_ACCURACY_GIT_REVISION");
        let dirty = match std::env::var("EMU198X_ACCURACY_GIT_DIRTY").as_deref() {
            Ok("true") => true,
            Ok("false") => false,
            _ => panic!("report-mode survey requires EMU198X_ACCURACY_GIT_DIRTY=true|false"),
        };
        let result = serde_json::json!({
            "schema": "org.198x.emu198x.c64-vicii-survey-producer.v1",
            "revision": revision,
            "dirty": dirty,
            "runtime_contract": {
                "model": "c64-pal-breadbin",
                "vic_model": "6569",
                "boot_frames": 150,
                "key_hold_frames": DEFAULT_KEY_HOLD_FRAMES,
                "inter_char_frames": DEFAULT_INTER_CHAR_FRAMES,
                "type_settle_frames": DEFAULT_TYPE_SETTLE_FRAMES,
                "settle_frames": 60,
                "program_load": "direct-prg-with-basic-vartab-update",
                "typed_command": "RUN\n",
                "framebuffer_width": FB_WIDTH,
                "framebuffer_height": FB_HEIGHT,
                "framebuffer": {
                    "width": FB_WIDTH,
                    "height": FB_HEIGHT,
                },
            },
            "comparison_contract": {
                "method": "nearest-c64-palette-index-squared-rgb-v1",
                "reference_width": 384,
                "reference_height": 272,
                "crop_x": VICE_CROP_X,
                "crop_y": VICE_CROP_Y,
                "crop": {
                    "x": VICE_CROP_X,
                    "y": VICE_CROP_Y,
                    "width": 384,
                    "height": 272,
                },
                "palette_argb": mos_vic_ii::palette::PALETTE,
                "assertion_boundary": "digital-colour-index-output-not-analogue-colour",
            },
            "cases": cases,
        });
        let encoded = serde_json::to_vec_pretty(&result)
            .expect("VIC-II survey producer result should encode");
        std::fs::write(&result_path, encoded).expect("VIC-II survey producer result should write");
        eprintln!("wrote structured VIC-II survey result");
    }
}

/// All five PAL 6569 colour-fetch-bug programs exactly match their registered
/// indexed reference planes. Together they cover the disconnected `$FF`
/// matrix byte, the CPU-side colour nibble, line-buffer leakage and the first
/// valid access after the three-cycle BA-to-AEC handover.
#[test]
#[ignore = "strict colour-fetch parity requires C64 ROMs + VIC-II testbench"]
fn colorfetchbug_cases_match_vice_references_exactly() {
    if !roms_present() || testbench_dir().is_none() {
        eprintln!("skip: C64 ROMs or testbench not staged");
        return;
    }
    let dir = testbench_dir().expect("checked");
    let cases: Vec<_> = SURVEY
        .iter()
        .filter(|(label, _, _)| label.starts_with("colorfetchbug"))
        .collect();
    assert_eq!(cases.len(), 5, "the strict lane must retain all five cases");

    for &&(label, prg, refpng) in &cases {
        let reference = decode_reference_png(&dir.join(refpng));
        let framebuffer = run_testprog(prg, 60);
        let comparison = compare_indexed(&framebuffer, &reference, VICE_CROP_X, VICE_CROP_Y);
        let total = comparison.reference.len();
        assert_eq!(
            comparison.matched_pixels, total,
            "{label} differs from its registered indexed reference plane"
        );
        assert_eq!(
            sha256_hex(&comparison.actual),
            sha256_hex(&comparison.reference),
            "{label} indexed-plane identity differs"
        );
    }
}

/// Fraction of a single reference row (`ry`) whose C64 colour index matches our
/// framebuffer when the reference is placed at (`dx`,`dy`).
fn row_match_fraction(fb: &[u32], reference: &RefImage, dx: u32, dy: u32, ry: u32) -> f64 {
    let mut matched = 0usize;
    for rx in 0..reference.width {
        let oi = ((dy + ry) * FB_WIDTH + (dx + rx)) as usize;
        let px = fb[oi];
        let ours = nearest_c64_index((px >> 16) as u8, (px >> 8) as u8, px as u8);
        let ri = ((ry * reference.width + rx) * 3) as usize;
        let theirs = nearest_c64_index(
            reference.rgb[ri],
            reference.rgb[ri + 1],
            reference.rgb[ri + 2],
        );
        if ours == theirs {
            matched += 1;
        }
    }
    matched as f64 / f64::from(reference.width)
}

/// Sequencer regression floor: on `spritedma` (the sprite-render anchor) the
/// draw-stage sequencer matches VICE's PAL 6569 reference to ≥99.9 % (it landed
/// at 99.998 % when it became the default). Guards against a future sprite-path
/// change silently regressing the anchor.
#[test]
#[ignore = "sequencer: spritedma floor vs VICE (requires ROMs + testbench)"]
fn sprite_sequencer_spritedma_parity() {
    if !roms_present() || testbench_dir().is_none() {
        eprintln!("skip: C64 ROMs or testbench not staged");
        return;
    }
    let dir = testbench_dir().expect("checked");
    let reference = decode_reference_png(&dir.join("spritedma/references/d017-54.prg.png"));
    let sequencer = run_testprog("spritedma/d017-54.prg", 60);

    let m_seq = match_fraction(&sequencer, &reference, VICE_CROP_X, VICE_CROP_Y);
    eprintln!("spritedma: sequencer {:.3}% vs VICE", m_seq * 100.0);

    assert!(
        m_seq >= 0.999,
        "sequencer regressed spritedma vs VICE: {:.3}% < 99.9%",
        m_seq * 100.0
    );
}

/// Per-scanline oracle for the sprite-chain rebuild. For the category named in
/// `VICII_DIFF_CAT` (default `sequencer-bug`), print the match % of every
/// reference row that falls below `VICII_DIFF_THRESH` (default 98 %), tagged
/// with the engine raster line it maps to. This localises *where* a sprite (or
/// any effect) diverges — height, position, a wrapped copy — so each chain step
/// can be pinned to specific rows instead of one aggregate number.
#[test]
#[ignore = "oracle: per-scanline match % vs VICE for VICII_DIFF_CAT (sprite-chain rebuild)"]
fn diff_by_row() {
    if !roms_present() || testbench_dir().is_none() {
        eprintln!("skip: C64 ROMs or testbench not staged");
        return;
    }
    let cat = std::env::var("VICII_DIFF_CAT").unwrap_or_else(|_| "sequencer-bug".to_string());
    let thresh: f64 = std::env::var("VICII_DIFF_THRESH")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.98);
    let Some(&(_, prg, refpng)) = SURVEY.iter().find(|(l, _, _)| *l == cat) else {
        eprintln!("unknown category {cat:?}; pick one from SURVEY");
        return;
    };
    let dir = testbench_dir().expect("checked");
    let reference = decode_reference_png(&dir.join(refpng));
    let fb = run_testprog(prg, 60);

    eprintln!(
        "\n=== {cat}: rows below {:.0}% (ref-y → engine line {}+ref-y) ===",
        thresh * 100.0,
        VICE_CROP_Y
    );
    let mut worst = (1.0f64, 0u32);
    let mut below = 0u32;
    for ry in 0..reference.height {
        let m = row_match_fraction(&fb, &reference, VICE_CROP_X, VICE_CROP_Y, ry);
        if m < worst.0 {
            worst = (m, ry);
        }
        if m < thresh {
            below += 1;
            eprintln!(
                "  ref-y {ry:3} (line {:3}): {:6.2}%",
                ry + VICE_CROP_Y,
                m * 100.0
            );
        }
    }
    eprintln!(
        "{below} of {} rows below threshold; worst = ref-y {} ({:.2}%)",
        reference.height,
        worst.1,
        worst.0 * 100.0
    );
}
