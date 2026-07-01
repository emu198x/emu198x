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
use common_commodore_c64::timing::TIMING_PAL_BREADBIN;
use emu198x_shell::HeadlessSession;
use mos_vic_ii::{FB_HEIGHT, FB_WIDTH};
use runtime_commodore_c64::{
    C64Runtime, C64SessionQueryProvider, DEFAULT_KEY_HOLD_FRAMES, DEFAULT_TYPE_SETTLE_FRAMES,
    Model, type_string,
};

/// `~/.emu198x/test-suites/c64-vicii/`, or `None` if not staged.
fn testbench_dir() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let path = PathBuf::from(home).join(".emu198x/test-suites/c64-vicii");
    path.exists().then_some(path)
}

/// True when the real C64 ROM set is staged locally.
fn roms_present() -> bool {
    std::env::var("HOME")
        .map(|h| {
            PathBuf::from(h)
                .join(".emu198x/roms/commodore-c64/kernal.rom")
                .exists()
        })
        .unwrap_or(false)
}

/// Boot real ROMs, load a testbench `.prg` (relative to the testbench dir),
/// RUN it, settle for `settle_frames`, and return the ARGB framebuffer.
fn run_testprog(rel_prg: &str, settle_frames: u32) -> Vec<u32> {
    run_testprog_opt(rel_prg, settle_frames, false)
}

/// As `run_testprog`, but `use_sequencer` selects the draw-stage sprite
/// sequencer over the geometry renderer (sequencer-port validation).
fn run_testprog_opt(rel_prg: &str, settle_frames: u32, use_sequencer: bool) -> Vec<u32> {
    let dir = testbench_dir().expect("testbench dir checked by caller");
    let prg = std::fs::read(dir.join(rel_prg)).expect("testbench .prg should read");

    let firmware = local_rom_firmware();
    let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
        .expect("real PAL C64 firmware should construct a runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_PAL_BREADBIN.cycles_per_frame),
        C64SessionQueryProvider,
    );
    // Set the sprite-render path explicitly on both branches: the sequencer
    // is now the shipping default, so the overlay A-side of the survey must
    // force it off rather than rely on the constructor default.
    session
        .machine_mut()
        .machine_mut()
        .set_sprite_sequencer_enabled(use_sequencer);

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
    let rgb = match info.color_type {
        png::ColorType::Rgb => buf,
        png::ColorType::Rgba => buf
            .chunks_exact(4)
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect(),
        other => panic!("unexpected reference PNG colour type: {other:?}"),
    };
    RefImage {
        width: info.width,
        height: info.height,
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
fn match_fraction(fb: &[u32], reference: &RefImage, dx: u32, dy: u32) -> f64 {
    let mut matched = 0usize;
    let total = (reference.width * reference.height) as usize;
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
            if ours == theirs {
                matched += 1;
            }
        }
    }
    matched as f64 / total as f64
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
    let fb = run_testprog_opt(&rel, 60, std::env::var("VICII_SEQ").is_ok());
    write_framebuffer_png("/tmp/vicii_dump.png", &fb);
    eprintln!("wrote /tmp/vicii_dump.png for {rel}");
}

/// Rewrite-relevant testbench categories: (label, program, reference PNG). One
/// canonical PAL 6569 program per category, all with 384x272 references.
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
    if !roms_present() || testbench_dir().is_none() {
        eprintln!("skip: C64 ROMs or testbench not staged");
        return;
    }
    let dir = testbench_dir().expect("checked");
    let use_seq = std::env::var("VICII_SEQ").is_ok();
    let mut rows: Vec<(f64, &str)> = Vec::new();
    for (label, prg, refpng) in SURVEY {
        let refpath = dir.join(refpng);
        if !refpath.exists() {
            eprintln!("{label:16} MISSING reference");
            continue;
        }
        let reference = decode_reference_png(&refpath);
        let fb = run_testprog_opt(prg, 60, use_seq);
        rows.push((
            match_fraction(&fb, &reference, VICE_CROP_X, VICE_CROP_Y),
            label,
        ));
    }
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    eprintln!("\n=== VICII survey vs VICE 6569 (sequencer={use_seq}; match %, worst first) ===");
    for (m, label) in &rows {
        eprintln!("{:7.3}%  {label}", m * 100.0);
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

/// Sequencer S2 parity gate: on `spritedma` (the parity anchor at 99.78 %), the
/// draw-stage sequencer must render essentially identically to the geometry
/// `overlay_sprites` — no per-row regression against VICE, and a near-identical
/// framebuffer. Reports both so a real divergence is visible, not just gated.
#[test]
#[ignore = "sequencer S2: spritedma parity (sequencer vs overlay vs VICE)"]
fn sprite_sequencer_spritedma_parity() {
    if !roms_present() || testbench_dir().is_none() {
        eprintln!("skip: C64 ROMs or testbench not staged");
        return;
    }
    let dir = testbench_dir().expect("checked");
    let reference = decode_reference_png(&dir.join("spritedma/references/d017-54.prg.png"));
    let overlay = run_testprog_opt("spritedma/d017-54.prg", 60, false);
    let sequencer = run_testprog_opt("spritedma/d017-54.prg", 60, true);

    let m_overlay = match_fraction(&overlay, &reference, VICE_CROP_X, VICE_CROP_Y);
    let m_seq = match_fraction(&sequencer, &reference, VICE_CROP_X, VICE_CROP_Y);
    let diff_px = overlay
        .iter()
        .zip(sequencer.iter())
        .filter(|(a, b)| a != b)
        .count();
    eprintln!(
        "spritedma: overlay {:.3}% vs VICE, sequencer {:.3}% vs VICE, {diff_px} px differ",
        m_overlay * 100.0,
        m_seq * 100.0
    );

    assert!(
        m_seq >= m_overlay - 0.0005,
        "sequencer regressed spritedma vs VICE: {:.3}% < {:.3}%",
        m_seq * 100.0,
        m_overlay * 100.0
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
    // VICII_SEQ enables the draw-stage sprite sequencer (sequencer-port work).
    let use_seq = std::env::var("VICII_SEQ").is_ok();
    let fb = run_testprog_opt(prg, 60, use_seq);

    eprintln!(
        "\n=== {cat} (sequencer={use_seq}): rows below {:.0}% (ref-y → engine line {}+ref-y) ===",
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
