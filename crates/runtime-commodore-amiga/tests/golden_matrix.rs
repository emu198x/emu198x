//! Amiga boot-path golden matrix.
//!
//! A table-driven regression net for Amiga boot screens. Each row is
//! a `(Model, Kickstart ROM, optional ADF, settle frame count)`
//! combination; the harness runs the machine for the settle frames,
//! captures Denise's framebuffer, crops to FS-UAE's default PAL
//! region, and byte-compares against a reference PNG on disk.
//!
//! # Framing
//!
//! Our emulator renders at **768×576** — full PAL Standard overscan,
//! the same region the runtime shows users. FS-UAE's default PAL
//! output crops 8 px each side horizontally and 2 scan-lines top and
//! bottom to **752×572**. Goldens are stored at the FS-UAE dimension
//! so they can be compared pixel-exactly against FS-UAE captures
//! (the ground truth for this suite). The harness applies the same
//! symmetric crop to our 768×576 output before comparison.
//!
//! # External artifacts
//!
//! The harness loads firmware and media from the user's private
//! emulator config dir so nothing licensed sits in the repo:
//!
//! - ROMs: `~/.emu198x/roms/commodore-amiga/<name>.rom`
//! - Disks: `~/.emu198x/media/commodore-amiga/<name>.adf`
//!
//! When a ROM or ADF is missing the row prints a skip marker and
//! returns. CI without the artifacts stays green; local runs with
//! the artifacts gate the harness against regressions.
//!
//! # Capturing / updating goldens
//!
//! Goldens are FS-UAE captures (the trusted reference) — see
//! `wiki/processes/golden-image-capture.md`. Don't regenerate with
//! `EMU198X_UPDATE_GOLDENS=1` unless you've verified the emulator
//! matches FS-UAE for that row; the env var is provided for the
//! bootstrap workflow but should almost never be used.
//!
//! On mismatch the harness writes two debug PNGs next to the
//! golden:
//!
//! - `<name>.actual.png` — the frame the emulator produced (cropped)
//! - `<name>.diff.png`   — a pixel mask highlighting differences
//!
//! Both are gitignored so they don't pollute the tree.
//!
//! # Phase 1 scope
//!
//! OCS only — A1000 / A500 / A500+A501 with Kickstart 1.2 and 1.3,
//! with and without a Workbench ADF inserted. Later phases extend
//! the matrix to ECS (A500+/A600), AGA (A1200/A4000), and HDD boot.

use std::path::{Path, PathBuf};

use format_commodore_amiga_adf::Adf;
use runtime_commodore_amiga::{
    A500_PAL_FRAME_TICKS, AmigaRuntime, DISPLAY_HEIGHT, DISPLAY_WIDTH, Model,
};

/// One row in the golden matrix.
struct GoldenRow {
    /// Short kebab-case name. Also the PNG filename stem.
    name: &'static str,
    /// Amiga model (carries RAM layout + profile metadata).
    model: Model,
    /// Kickstart ROM filename under `~/.emu198x/roms/commodore-amiga/`.
    kickstart: &'static str,
    /// Optional ADF filename under `~/.emu198x/media/commodore-amiga/`.
    /// `None` = boot with DF0 empty (insert-disk screen).
    disk: Option<&'static str>,
    /// PAL frames to tick before capturing the frame. For no-disk
    /// paths this is "boot settled on the insert-disk screen"; for
    /// disk-boot paths it's "Workbench (or game) has rendered".
    settle_frames: u64,
}

/// FS-UAE's default PAL crop — 8 px each side horizontally, 2
/// scan-lines top and bottom of our 768×576 PAL Standard frame.
const FSUAE_W: u32 = 752;
const FSUAE_H: u32 = 572;

/// Settle-frame count matching the archive's FS-UAE captures. All
/// archive goldens were taken at frame 250 for KS 1.2 / 1.3 on the
/// insert-disk screen.
const KS13_SETTLE_FRAMES: u64 = 250;

const MATRIX: &[GoldenRow] = &[
    GoldenRow {
        name: "a500-ks13-no-disk",
        model: Model::A500OcsPal,
        kickstart: "kick13.rom",
        disk: None,
        settle_frames: KS13_SETTLE_FRAMES,
    },
    GoldenRow {
        name: "a500-ks13-a501-no-disk",
        model: Model::A500OcsPalA501,
        kickstart: "kick13.rom",
        disk: None,
        settle_frames: KS13_SETTLE_FRAMES,
    },
    GoldenRow {
        name: "a500-ks13-wb13",
        model: Model::A500OcsPalA501,
        kickstart: "kick13.rom",
        disk: Some("workbench-1.3.adf"),
        // Workbench 1.3 needs more time than the insert-disk screen
        // to reach its prompt — picked empirically during capture.
        settle_frames: 900,
    },
    GoldenRow {
        name: "a1000-ks12-no-disk",
        model: Model::A1000OcsPal,
        kickstart: "kick12.rom",
        disk: None,
        settle_frames: KS13_SETTLE_FRAMES,
    },
];

fn roms_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".emu198x/roms/commodore-amiga"))
}

fn media_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".emu198x/media/commodore-amiga"))
}

fn goldens_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/goldens")
}

/// `true` when the harness should rewrite the golden PNG instead of
/// asserting on a mismatch. Gated on the `EMU198X_UPDATE_GOLDENS`
/// env var so day-to-day runs are strict.
fn update_mode() -> bool {
    std::env::var_os("EMU198X_UPDATE_GOLDENS").is_some_and(|v| !v.is_empty())
}

/// Read a single file, returning `None` with a skip message if
/// absent. Used for both ROMs and ADFs.
fn load_optional_artifact(path: &Path, kind: &str, row: &str) -> Option<Vec<u8>> {
    if !path.exists() {
        eprintln!(
            "skipping {row}: {kind} missing at {}",
            path.display()
        );
        return None;
    }
    Some(std::fs::read(path).unwrap_or_else(|e| {
        panic!("read {kind} at {}: {e}", path.display())
    }))
}

/// Tick the runtime for `frames` PAL frames. Uses the machine's
/// `tick()` directly rather than `run_until` so we bypass the
/// frame-sink plumbing — the test only cares about the final
/// framebuffer, not per-frame emission.
fn tick_frames(rt: &mut AmigaRuntime, frames: u64) {
    for _ in 0..(frames * A500_PAL_FRAME_TICKS) {
        rt.machine_mut().tick();
    }
}

/// Capture the Denise framebuffer, crop to FS-UAE's default PAL
/// region, and return the cropped bytes as RGB (no alpha — FS-UAE
/// reference PNGs are RGB, so goldens and diffs share the format).
///
/// The crop is a centered `(DISPLAY_WIDTH - FSUAE_W) / 2`-pixel
/// horizontal trim and `(DISPLAY_HEIGHT - FSUAE_H) / 2`-scanline
/// vertical trim, discarding the outer PAL overscan border that
/// FS-UAE doesn't show.
fn capture_fsuae_rgb(rt: &AmigaRuntime) -> Vec<u8> {
    let fb = rt.machine().denise().framebuffer();
    assert_eq!(fb.len(), (DISPLAY_WIDTH * DISPLAY_HEIGHT) as usize);
    let x_off = (DISPLAY_WIDTH - FSUAE_W) / 2;
    let y_off = (DISPLAY_HEIGHT - FSUAE_H) / 2;
    let mut rgb = Vec::with_capacity((FSUAE_W * FSUAE_H * 3) as usize);
    for y in 0..FSUAE_H {
        let src_row = y_off + y;
        let row_start = (src_row * DISPLAY_WIDTH + x_off) as usize;
        for x in 0..FSUAE_W as usize {
            let pixel = fb[row_start + x];
            rgb.push(((pixel >> 16) & 0xFF) as u8); // R
            rgb.push(((pixel >> 8) & 0xFF) as u8); // G
            rgb.push((pixel & 0xFF) as u8); // B
        }
    }
    rgb
}

/// Encode RGB bytes (no alpha) as a PNG blob. Matches the FS-UAE
/// reference goldens' pixel format so byte-compare is meaningful.
fn encode_rgb_png(rgb: &[u8], w: u32, h: u32) -> Vec<u8> {
    assert_eq!(rgb.len(), (w * h * 3) as usize);
    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, w, h);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("png write header");
        writer.write_image_data(rgb).expect("png write data");
    }
    png_bytes
}

/// Write a pixel-level diff mask highlighting differences between
/// `actual` and `expected` RGB frames. Matches: black. Mismatches:
/// red. Kept small and cheap — it's a debug artefact, not a user-
/// facing diff tool.
fn write_diff_mask(
    path: &Path,
    actual_rgb: &[u8],
    expected_rgb: &[u8],
    w: u32,
    h: u32,
) {
    assert_eq!(actual_rgb.len(), expected_rgb.len());
    let mut mask = Vec::with_capacity(actual_rgb.len());
    for (a, e) in actual_rgb.chunks_exact(3).zip(expected_rgb.chunks_exact(3)) {
        if a == e {
            mask.extend_from_slice(&[0, 0, 0]);
        } else {
            mask.extend_from_slice(&[0xFF, 0, 0]);
        }
    }
    let file = std::fs::File::create(path).expect("create diff mask");
    let mut encoder = png::Encoder::new(file, w, h);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("diff mask header");
    writer.write_image_data(&mask).expect("diff mask data");
}

/// Decode a PNG file into raw RGB pixel bytes, normalising whatever
/// colour format the file happens to be in (RGBA → drop alpha,
/// palette → expand). Panics on I/O or decode failure — both are
/// unrecoverable from a test.
fn decode_png_rgb(path: &Path) -> (Vec<u8>, u32, u32) {
    let file = std::fs::File::open(path).expect("open golden");
    let decoder = png::Decoder::new(file);
    let mut reader = decoder.read_info().expect("png read info");
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("png next frame");
    buf.truncate(info.buffer_size());
    let rgb = match info.color_type {
        png::ColorType::Rgb => buf,
        png::ColorType::Rgba => buf
            .chunks_exact(4)
            .flat_map(|c| [c[0], c[1], c[2]])
            .collect(),
        other => panic!(
            "unsupported golden colour type {:?} at {}",
            other,
            path.display()
        ),
    };
    (rgb, info.width, info.height)
}

/// Run one row end-to-end: build runtime, tick to settle, compare
/// frame against golden, or rewrite golden in update mode.
///
/// Skip (return early) cases: missing ROM, missing disk. Failure
/// cases: byte mismatch in strict mode.
fn run_row(row: &GoldenRow) {
    let Some(roms) = roms_dir() else {
        eprintln!("skipping {}: $HOME not set", row.name);
        return;
    };
    let rom_path = roms.join(row.kickstart);
    let Some(rom_bytes) = load_optional_artifact(&rom_path, "Kickstart ROM", row.name)
    else {
        return;
    };

    let adf_bytes = if let Some(disk) = row.disk {
        let Some(media) = media_dir() else {
            eprintln!("skipping {}: $HOME not set", row.name);
            return;
        };
        let disk_path = media.join(disk);
        let Some(bytes) = load_optional_artifact(&disk_path, "ADF", row.name) else {
            return;
        };
        Some(bytes)
    } else {
        None
    };

    let mut rt = AmigaRuntime::new(row.model, rom_bytes)
        .unwrap_or_else(|e| panic!("{}: build runtime: {e:?}", row.name));
    if let Some(bytes) = adf_bytes {
        let adf = Adf::from_bytes(bytes)
            .unwrap_or_else(|e| panic!("{}: decode ADF: {e}", row.name));
        rt.machine_mut().insert_adf(adf);
    }

    tick_frames(&mut rt, row.settle_frames);
    let actual_rgb = capture_fsuae_rgb(&rt);

    let golden_path = goldens_dir().join(format!("{}.png", row.name));
    std::fs::create_dir_all(goldens_dir()).expect("create goldens dir");

    if update_mode() {
        let png = encode_rgb_png(&actual_rgb, FSUAE_W, FSUAE_H);
        std::fs::write(&golden_path, &png)
            .unwrap_or_else(|e| panic!("write golden {}: {e}", golden_path.display()));
        eprintln!(
            "wrote golden for {} ({} bytes) at {}",
            row.name,
            png.len(),
            golden_path.display()
        );
        return;
    }

    if !golden_path.exists() {
        // First-run safety: goldens come from FS-UAE (see module
        // doc). Don't invent one silently.
        panic!(
            "{}: golden missing at {}. \
             See wiki/processes/golden-image-capture.md.",
            row.name,
            golden_path.display()
        );
    }

    let (expected_rgb, gw, gh) = decode_png_rgb(&golden_path);
    if gw != FSUAE_W || gh != FSUAE_H {
        panic!(
            "{}: golden dimensions {}×{} do not match FS-UAE {}×{} at {}",
            row.name,
            gw,
            gh,
            FSUAE_W,
            FSUAE_H,
            golden_path.display()
        );
    }
    if actual_rgb == expected_rgb {
        return;
    }

    // Mismatch: dump actual + diff mask and panic with pointers.
    let actual_path = goldens_dir().join(format!("{}.actual.png", row.name));
    std::fs::write(
        &actual_path,
        encode_rgb_png(&actual_rgb, FSUAE_W, FSUAE_H),
    )
    .unwrap_or_else(|e| panic!("write actual {}: {e}", actual_path.display()));

    let diff_path = goldens_dir().join(format!("{}.diff.png", row.name));
    write_diff_mask(&diff_path, &actual_rgb, &expected_rgb, FSUAE_W, FSUAE_H);

    let total_px = (FSUAE_W * FSUAE_H) as usize;
    let differing = actual_rgb
        .chunks_exact(3)
        .zip(expected_rgb.chunks_exact(3))
        .filter(|(a, e)| a != e)
        .count();

    panic!(
        "{}: framebuffer doesn't match golden ({}/{} px differ, {:.1}%).\n  \
         golden:  {}\n  actual:  {}\n  diff:    {}",
        row.name,
        differing,
        total_px,
        100.0 * differing as f64 / total_px as f64,
        golden_path.display(),
        actual_path.display(),
        diff_path.display()
    );
}

// The KS 1.3 insert-disk rows are pixel-exact against FS-UAE
// captures. A1000 KS 1.2 still fails — that's a separate boot bug
// (task #188, golden itself is all-black so the symptom differs).
// Run `cargo test -- --ignored` to see the A1000 diff while #188
// is outstanding.

#[test]
fn a500_ks13_no_disk() {
    run_row(&MATRIX[0]);
}

#[test]
fn a500_ks13_a501_no_disk() {
    run_row(&MATRIX[1]);
}

#[test]
#[ignore = "task #189: WB 1.3 boot stops at Exec idle loop before Intuition renders"]
fn a500_ks13_wb13() {
    // ADF is in place at ~/.emu198x/media/commodore-amiga/workbench-1.3.adf
    // and the bootblock now decodes correctly (DOS\0 magic visible
    // in chip RAM after DMA), but the CPU ends up in Kickstart's
    // Exec scheduler idle loop at $FC0F94 — dos.library / LoadSeg
    // isn't triggering the follow-up disk reads that build
    // Workbench. Un-ignore once the boot reaches the desktop.
    run_row(&MATRIX[2]);
}

#[test]
#[ignore = "task #188: A1000 KS 1.2 boot produces all-black golden — separate bug"]
fn a1000_ks12_no_disk() {
    run_row(&MATRIX[3]);
}
