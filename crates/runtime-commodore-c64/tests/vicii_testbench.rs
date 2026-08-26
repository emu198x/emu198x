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
use mos_vic_ii::{FB_HEIGHT, FB_WIDTH, oracle::engine_to_canonical};
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
    let mut session = prepare_testprog_on(rel_prg, model, cycles_per_frame);
    session
        .run_frames(settle_frames)
        .expect("settle frames should run");

    session.machine_mut().machine_mut().framebuffer().to_vec()
}

/// Boot, load and start one testbench program without consuming its requested
/// observation interval.
fn prepare_testprog_on(
    rel_prg: &str,
    model: Model,
    cycles_per_frame: u32,
) -> HeadlessSession<C64Runtime, C64SessionQueryProvider> {
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
    // Formatted by hand rather than with `{:x}`. RustCrypto's digest output
    // stopped implementing `LowerHex` when it moved from `GenericArray` to
    // `hybrid-array`, so the format string fails to compile on sha2 0.11.
    // Iterating the bytes works on both, which keeps this independent of
    // which version is pinned.
    use std::fmt::Write as _;
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

/// Calibration: dump our `gfxfetch` framebuffer and search for the crop offset
/// that best matches VICE's 384x272 reference, reporting the best alignment +
/// match fraction. Drives the constants baked into the real comparison test.
#[test]
#[ignore = "DIAGNOSTIC: calibration aid: derives crop offset + palette match vs VICE reference"]
fn calibrate_gfxfetch_alignment() {
    if !roms_present() || testbench_dir().is_none() {
        emu198x_test_skip::skip!("C64 ROMs or VIC-II testbench not staged");
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
#[ignore = "DIAGNOSTIC: calibration aid: NTSC crop offset + match vs VICE 6567R8 reference"]
fn calibrate_ntsc_gfxfetch_alignment() {
    if !roms_present() || testbench_dir().is_none() {
        emu198x_test_skip::skip!("C64 ROMs or VIC-II testbench not staged");
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
#[ignore = "FIXTURE: requires ~/.emu198x/roms/commodore-c64 + ~/.emu198x/test-suites/c64-vicii"]
fn ntsc_gfxfetch_matches_vice_reference() {
    if !roms_present() || testbench_dir().is_none() {
        emu198x_test_skip::skip!("C64 ROMs or VIC-II testbench not staged");
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
#[ignore = "FIXTURE: requires ~/.emu198x/roms/commodore-c64 + ~/.emu198x/test-suites/c64-vicii"]
fn gfxfetch_matches_vice_reference() {
    if !roms_present() || testbench_dir().is_none() {
        emu198x_test_skip::skip!("C64 ROMs or VIC-II testbench not staged");
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
#[ignore = "DIAGNOSTIC: diagnostic: dumps framebuffer for the VICII_DUMP_PRG program"]
fn dump_prg_framebuffer() {
    if !roms_present() || testbench_dir().is_none() {
        emu198x_test_skip::skip!("C64 ROMs or VIC-II testbench not staged");
    }
    let Ok(rel) = std::env::var("VICII_DUMP_PRG") else {
        eprintln!("set VICII_DUMP_PRG=<category/name.prg>");
        return;
    };
    let fb = run_testprog(&rel, 60);
    write_framebuffer_png("/tmp/vicii_dump.png", &fb);
    eprintln!("wrote /tmp/vicii_dump.png for {rel}");
}

/// Cycle-vocabulary regression for the two `$D011` stores in
/// `sequencer-bug`. Scheduled CPU pins, the entering VIC phase, the post-VIC
/// CPU access phase and VICE's store-watchpoint timestamp are intentionally
/// separate: comparing them as if they were one phase creates false deltas.
#[test]
#[ignore = "DIAGNOSTIC: diagnostic: pins sequencer-bug D011 writes to the VIC-II cycle boundary"]
fn sequencer_bug_d011_write_cycle_boundary() {
    if !roms_present() || testbench_dir().is_none() {
        emu198x_test_skip::skip!("C64 ROMs or VIC-II testbench not staged");
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Position(u16, u8, u8);
    let pos = |line, cycle| Position(line, cycle, engine_to_canonical(cycle));

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct ExecPhase {
        pc: u16,
        scheduled_pins: Position,
        cpu_access_phase: Position,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct D011Write {
        value: u8,
        scheduled_pins: Position,
        vic_phase_consumed: Position,
        cpu_access_phase: Position,
        vice_monitor_observed: Position,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct BusTransition {
        vic_phase: Position,
        ba_low: bool,
        aec_low: bool,
        badline: bool,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct StallSample {
        vic_phase: Position,
        addr: u16,
        sync: bool,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct SourceSample {
        vic_phase: Position,
        badline_ba_low: bool,
        sprite_ba_low: bool,
        c_access_active: bool,
    }
    let exec = |pc, line, scheduled, access| ExecPhase {
        pc,
        scheduled_pins: pos(line, scheduled),
        cpu_access_phase: pos(line, access),
    };
    let bus = |line, cycle, ba_low, aec_low, badline| BusTransition {
        vic_phase: pos(line, cycle),
        ba_low,
        aec_low,
        badline,
    };
    let source = |line, cycle, badline_ba_low, sprite_ba_low, c_access_active| SourceSample {
        vic_phase: pos(line, cycle),
        badline_ba_low,
        sprite_ba_low,
        c_access_active,
    };

    let mut session = prepare_testprog_on(
        "sequencer-bug/bug.prg",
        Model::C64PalBreadbin,
        TIMING_PAL_BREADBIN.cycles_per_frame,
    );
    session.run_frames(60).expect("steady raster loop");

    let mut execs = Vec::new();
    let mut writes = Vec::new();
    let mut transitions = Vec::new();
    let mut stalls = Vec::new();
    let mut source_samples = Vec::new();
    let mut trace_bus = false;
    for _ in 0..TIMING_PAL_BREADBIN.cycles_per_frame {
        let machine = session.machine_mut().machine_mut();
        let cpu = machine.cpu();
        let before = pos(machine.raster_line(), machine.cycle_in_line());
        let addr = cpu.addr;
        let value = cpu.data;
        let rw = cpu.rw;
        let sync = cpu.sync;
        let cpu_cycles = cpu.total_cycles;
        let ba_low = machine.vic().ba_is_low();
        let aec_low = machine.vic().aec_is_low();
        let target_exec =
            sync && matches!(addr, 0x0941 | 0x096D | 0x0994 | 0x09B7 | 0x09BA | 0x09CE);
        let target_write =
            !rw && addr == 0xD011 && matches!((before.0, value), (51, 0x3B) | (53, 0x3C));
        if sync && addr == 0x0994 {
            trace_bus = true;
        }

        machine.tick();
        let after = pos(machine.raster_line(), machine.cycle_in_line());
        let cpu_advanced = machine.cpu().total_cycles != cpu_cycles;

        if target_exec && cpu_advanced {
            execs.push(ExecPhase {
                pc: addr,
                scheduled_pins: before,
                cpu_access_phase: after,
            });
        }
        if target_write {
            let vice_monitor_observed = if value == 0x3B {
                pos(51, 54)
            } else {
                pos(53, 55)
            };
            writes.push(D011Write {
                value,
                scheduled_pins: before,
                vic_phase_consumed: before,
                cpu_access_phase: after,
                vice_monitor_observed,
            });
        }
        if trace_bus {
            let vic = machine.vic();
            if before.0 == 51 && (50..=58).contains(&before.1) {
                source_samples.push(SourceSample {
                    vic_phase: before,
                    badline_ba_low: vic.badline_ba_is_low(),
                    sprite_ba_low: vic.sprite_ba_is_low(),
                    c_access_active: vic.c_access_is_active(),
                });
            }
            if vic.ba_is_low() != ba_low || vic.aec_is_low() != aec_low {
                transitions.push(BusTransition {
                    vic_phase: before,
                    ba_low: vic.ba_is_low(),
                    aec_low: vic.aec_is_low(),
                    badline: vic.is_badline(),
                });
            }
            if !cpu_advanced {
                stalls.push(StallSample {
                    vic_phase: before,
                    addr,
                    sync,
                });
            }
        }
        if trace_bus && sync && addr == 0x0A04 && cpu_advanced {
            break;
        }
    }

    assert_eq!(
        execs,
        vec![
            exec(0x0941, 48, 8, 9),
            exec(0x096D, 49, 8, 9),
            exec(0x0994, 50, 0, 1),
            exec(0x09B7, 50, 53, 54),
            exec(0x09BA, 51, 13, 14),
            exec(0x09CE, 51, 49, 50),
        ],
        "steady handler and pre-write CPU phases should match VICE 3.10"
    );
    assert_eq!(
        writes,
        vec![
            D011Write {
                value: 0x3B,
                scheduled_pins: pos(51, 52),
                vic_phase_consumed: pos(51, 52),
                cpu_access_phase: pos(51, 53),
                vice_monitor_observed: pos(51, 54),
            },
            D011Write {
                value: 0x3C,
                scheduled_pins: pos(53, 54),
                vic_phase_consumed: pos(53, 54),
                cpu_access_phase: pos(53, 55),
                vice_monitor_observed: pos(53, 55),
            },
        ]
    );

    // The first store's c52 pins, c53 access and VICE c54 watchpoint are one
    // execution event under three observation conventions. The second store is
    // post-VIC access and VICE's next-opcode/store checkpoint agree at c55
    // after the forced badline's remaining DMA window is constrained.
    assert_eq!(transitions.len(), 14);
    assert_eq!(
        transitions,
        &[
            bus(50, 55, true, false, false),
            bus(50, 58, true, true, false),
            bus(51, 11, false, false, false),
            bus(51, 53, true, false, true),
            bus(51, 54, false, false, true),
            bus(51, 55, true, false, true),
            bus(51, 58, true, true, true),
            bus(52, 11, false, false, false),
            bus(52, 55, true, false, false),
            bus(52, 58, true, true, false),
            bus(53, 11, false, false, false),
            bus(53, 55, true, false, false),
            bus(53, 58, true, true, false),
            bus(54, 11, false, false, false),
        ]
    );
    assert_eq!(
        source_samples,
        vec![
            source(51, 50, false, false, false),
            source(51, 51, false, false, false),
            source(51, 52, false, false, false),
            source(51, 53, true, false, true),
            source(51, 54, false, false, false),
            source(51, 55, false, true, false),
            source(51, 56, false, true, false),
            source(51, 57, false, true, false),
            source(51, 58, false, true, false),
        ]
    );
    assert_eq!(stalls.len(), 77);
    assert_eq!(
        (stalls[0].vic_phase, stalls[18].vic_phase),
        (pos(50, 55), pos(51, 10))
    );
    assert!(stalls[..19].iter().all(|s| s.addr == 0x09B9 && !s.sync));
    assert_eq!(
        stalls[19],
        StallSample {
            vic_phase: pos(51, 53),
            addr: 0x09D1,
            sync: true,
        }
    );
    assert_eq!(
        (stalls[20].vic_phase, stalls[38].vic_phase),
        (pos(51, 55), pos(52, 10))
    );
    assert!(stalls[20..39].iter().all(|s| s.addr == 0x09D2 && !s.sync));
    assert_eq!(
        (stalls[39].vic_phase, stalls[57].vic_phase),
        (pos(52, 55), pos(53, 10))
    );
    assert!(stalls[39..58].iter().all(|s| s.addr == 0x09EC && s.sync));
    assert_eq!(
        (stalls[58].vic_phase, stalls[76].vic_phase),
        (pos(53, 55), pos(54, 10))
    );
    assert!(stalls[58..].iter().all(|s| s.addr == 0x0A04 && s.sync));
}

/// Diagnostic: report every CPU bus write to `$D020` during one settled frame
/// of `VICII_D020_PRG` (default `colorfetchbug/main.prg`). RMW instructions
/// drive the unmodified byte and then the modified byte on consecutive Phi2
/// cycles, so inspecting only register changes hides half the timing evidence.
#[test]
#[ignore = "DIAGNOSTIC: diagnostic: reports D020 CPU bus phases"]
fn d020_write_cycle_boundary() {
    if !roms_present() || testbench_dir().is_none() {
        emu198x_test_skip::skip!("C64 ROMs or VIC-II testbench not staged");
    }

    let rel =
        std::env::var("VICII_D020_PRG").unwrap_or_else(|_| "colorfetchbug/main.prg".to_owned());
    let mut session = prepare_testprog_on(
        &rel,
        Model::C64PalBreadbin,
        TIMING_PAL_BREADBIN.cycles_per_frame,
    );
    session.run_frames(60).expect("steady raster loop");

    let mut writes = Vec::new();
    for _ in 0..TIMING_PAL_BREADBIN.cycles_per_frame {
        let machine = session.machine_mut().machine_mut();
        let cpu = machine.cpu();
        let before_line = machine.raster_line();
        let before_cycle = machine.cycle_in_line();
        let addr = cpu.addr;
        let value = cpu.data;
        let rw = cpu.rw;
        let pc = cpu.regs.pc;
        let sync = cpu.sync;
        let total_cycles = cpu.total_cycles;

        machine.tick();
        let advanced = machine.cpu().total_cycles != total_cycles;
        if !rw && addr == 0xD020 && advanced {
            writes.push((
                before_line,
                before_cycle,
                engine_to_canonical(before_cycle),
                value & 0x0F,
                pc,
                sync,
            ));
        }
    }

    eprintln!("D020 writes (line, engine cycle, canonical cycle, value, pc, sync):");
    for write in &writes {
        eprintln!("{write:?}");
    }
    assert!(
        !writes.is_empty(),
        "settled VIC-II diagnostic frame should write D020"
    );
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
#[ignore = "DIAGNOSTIC: survey aid: measures match % across VICII testbench categories"]
fn survey_testbench_categories() {
    let result_path = std::env::var("EMU198X_C64_VICII_SURVEY_RESULT")
        .ok()
        .map(PathBuf::from);
    if !roms_present() || testbench_dir().is_none() {
        assert!(
            result_path.is_none(),
            "report-mode VIC-II survey requires the configured C64 ROM set and testbench"
        );
        emu198x_test_skip::skip!("C64 ROMs or VIC-II testbench not staged");
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
#[ignore = "FIXTURE: strict colour-fetch parity requires C64 ROMs + VIC-II testbench"]
fn colorfetchbug_cases_match_vice_references_exactly() {
    if !roms_present() || testbench_dir().is_none() {
        emu198x_test_skip::skip!("C64 ROMs or VIC-II testbench not staged");
    }
    let dir = testbench_dir().expect("checked");
    let cases: Vec<_> = SURVEY
        .iter()
        .filter(|(label, _, _)| label.starts_with("colorfetchbug"))
        .collect();
    assert_eq!(cases.len(), 5, "the strict lane must retain all five cases");
    let mut failures = Vec::new();

    for &&(label, prg, refpng) in &cases {
        let reference = decode_reference_png(&dir.join(refpng));
        let framebuffer = run_testprog(prg, 60);
        let comparison = compare_indexed(&framebuffer, &reference, VICE_CROP_X, VICE_CROP_Y);
        let total = comparison.reference.len();
        let actual_hash = sha256_hex(&comparison.actual);
        let reference_hash = sha256_hex(&comparison.reference);
        if comparison.matched_pixels != total || actual_hash != reference_hash {
            failures.push(format!(
                "{label}: {}/{} pixels, actual {actual_hash}, reference {reference_hash}",
                comparison.matched_pixels, total
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "colour-fetch exact lane disagrees:\n{}",
        failures.join("\n")
    );
}

/// The far-edge `$D011` C-data carry leaves two characterised residuals: one
/// eight-row character at the direct renderer's unresolved G-access/output
/// boundary, plus two dot-zero colour-register transitions that require the
/// PAL 6569 colour-resolution ring. Keep both shapes exact so an unrelated
/// timing change cannot trade one disagreement for another.
#[test]
#[ignore = "FIXTURE: strict sequencer-bug parity requires C64 ROMs + VIC-II testbench"]
fn sequencer_bug_retains_only_the_known_pipeline_disagreements() {
    if !roms_present() || testbench_dir().is_none() {
        emu198x_test_skip::skip!("C64 ROMs or VIC-II testbench not staged");
    }
    let dir = testbench_dir().expect("checked");
    let reference = decode_reference_png(&dir.join("sequencer-bug/references/bug.prg.png"));
    let framebuffer = run_testprog("sequencer-bug/bug.prg", 60);
    let comparison = compare_indexed(&framebuffer, &reference, VICE_CROP_X, VICE_CROP_Y);

    let mismatches: Vec<_> = comparison
        .actual
        .iter()
        .zip(&comparison.reference)
        .enumerate()
        .filter(|(_, (actual, expected))| actual != expected)
        .map(|(index, (&actual, &expected))| {
            let index = index as u32;
            (
                index % reference.width,
                index / reference.width,
                actual,
                expected,
            )
        })
        .collect();
    let mut expected = vec![(32, 34, 11, 12), (64, 34, 12, 11)];
    for x in 32..=39 {
        expected.push((x, 36, 6, 15));
    }
    for y in 37..=42 {
        expected.push((32, y, 6, 15));
        expected.push((39, y, 6, 15));
    }
    for x in 32..=39 {
        expected.push((x, 43, 6, 15));
    }
    expected.sort_unstable();
    let mut mismatches = mismatches;
    mismatches.sort_unstable();
    assert_eq!(
        mismatches, expected,
        "sequencer-bug must retain only the characterised G/output and colour-ring residuals"
    );
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
#[ignore = "FIXTURE: sequencer: spritedma floor vs VICE (requires ROMs + testbench)"]
fn sprite_sequencer_spritedma_parity() {
    if !roms_present() || testbench_dir().is_none() {
        emu198x_test_skip::skip!("C64 ROMs or VIC-II testbench not staged");
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
#[ignore = "DIAGNOSTIC: oracle: per-scanline match % vs VICE for VICII_DIFF_CAT (sprite-chain rebuild)"]
fn diff_by_row() {
    if !roms_present() || testbench_dir().is_none() {
        emu198x_test_skip::skip!("C64 ROMs or VIC-II testbench not staged");
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
    let comparison = compare_indexed(&fb, &reference, VICE_CROP_X, VICE_CROP_Y);
    eprintln!(
        "aggregate match: {:.3}%",
        comparison.matched_pixels as f64 / comparison.reference.len() as f64 * 100.0
    );
    let mismatch_count = comparison.reference.len() - comparison.matched_pixels;
    if mismatch_count <= 128 {
        let mismatches: Vec<_> = comparison
            .actual
            .iter()
            .zip(&comparison.reference)
            .enumerate()
            .filter_map(|(index, (&actual, &expected))| {
                (actual != expected).then_some((
                    index as u32 % reference.width,
                    index as u32 / reference.width,
                    actual,
                    expected,
                ))
            })
            .collect();
        eprintln!("indexed mismatches (x, y, actual, expected): {mismatches:?}");
    }

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
