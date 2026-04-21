//! Amiga boot-path golden matrix.
//!
//! A table-driven regression net for Amiga boot screens. Each row is
//! a `(Model, Kickstart ROM, optional ADF, settle frame count)`
//! combination; the harness runs the machine for the settle frames,
//! captures Denise's framebuffer as a PNG, and byte-compares it
//! against a golden on disk under `tests/goldens/`.
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
//! Run with `EMU198X_UPDATE_GOLDENS=1` to (re)write the PNG for any
//! row whose golden is missing or doesn't match the current output.
//! Without that env var the harness runs in strict replay mode and
//! fails on any byte-level mismatch. On failure it also writes two
//! debug PNGs next to the golden:
//!
//! - `<name>.actual.png` — the frame the emulator produced
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

const MATRIX: &[GoldenRow] = &[
    GoldenRow {
        name: "a500-ks13-no-disk",
        model: Model::A500OcsPal,
        kickstart: "kick13.rom",
        disk: None,
        settle_frames: 300,
    },
    GoldenRow {
        name: "a500-ks13-a501-no-disk",
        model: Model::A500OcsPalA501,
        kickstart: "kick13.rom",
        disk: None,
        settle_frames: 300,
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
        settle_frames: 300,
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

/// Encode the current Denise framebuffer as a standalone PNG.
/// Uses the ARGB u32 buffer directly (not the RGBA byte mirror on
/// the runtime) so each golden is deterministic regardless of
/// whether `run_until` has been called.
fn encode_framebuffer_png(rt: &AmigaRuntime) -> Vec<u8> {
    let fb = rt.machine().denise().framebuffer();
    let w = DISPLAY_WIDTH;
    let h = DISPLAY_HEIGHT;
    assert_eq!(fb.len(), (w * h) as usize);

    let mut rgba = Vec::with_capacity(fb.len() * 4);
    for pixel in fb {
        rgba.push(((pixel >> 16) & 0xFF) as u8); // R
        rgba.push(((pixel >> 8) & 0xFF) as u8); // G
        rgba.push((pixel & 0xFF) as u8); // B
        rgba.push(((pixel >> 24) & 0xFF) as u8); // A
    }

    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, w, h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("png write header");
        writer.write_image_data(&rgba).expect("png write data");
    }
    png_bytes
}

/// Write a pixel-level diff mask highlighting differences between
/// `actual` and `expected` RGBA frames. Matches: fully transparent.
/// Mismatches: opaque red. Kept small and cheap — it's a debug
/// artefact, not a user-facing diff tool.
fn write_diff_mask(
    path: &Path,
    actual_rgba: &[u8],
    expected_rgba: &[u8],
    w: u32,
    h: u32,
) {
    assert_eq!(actual_rgba.len(), expected_rgba.len());
    let mut mask = Vec::with_capacity(actual_rgba.len());
    for (a, e) in actual_rgba.chunks_exact(4).zip(expected_rgba.chunks_exact(4)) {
        if a == e {
            mask.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            mask.extend_from_slice(&[0xFF, 0, 0, 0xFF]);
        }
    }
    let file = std::fs::File::create(path).expect("create diff mask");
    let mut encoder = png::Encoder::new(file, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("diff mask header");
    writer.write_image_data(&mask).expect("diff mask data");
}

/// Decode a PNG file back into its raw RGBA pixel bytes, for diff-
/// mask generation. Panics on I/O or decode failure — both are
/// unrecoverable from a test.
fn decode_png_rgba(path: &Path) -> Vec<u8> {
    let file = std::fs::File::open(path).expect("open golden");
    let decoder = png::Decoder::new(file);
    let mut reader = decoder.read_info().expect("png read info");
    let mut buf = vec![0; reader.output_buffer_size()];
    reader.next_frame(&mut buf).expect("png next frame");
    buf.truncate(reader.output_buffer_size());
    buf
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
    let actual_png = encode_framebuffer_png(&rt);

    let golden_path = goldens_dir().join(format!("{}.png", row.name));
    std::fs::create_dir_all(goldens_dir()).expect("create goldens dir");

    if update_mode() {
        std::fs::write(&golden_path, &actual_png)
            .unwrap_or_else(|e| panic!("write golden {}: {e}", golden_path.display()));
        eprintln!(
            "wrote golden for {} ({} bytes) at {}",
            row.name,
            actual_png.len(),
            golden_path.display()
        );
        return;
    }

    if !golden_path.exists() {
        // First-run safety: tell the caller to capture, don't
        // invent a golden silently.
        panic!(
            "{}: golden missing at {}. \
             Re-run with EMU198X_UPDATE_GOLDENS=1 to create it.",
            row.name,
            golden_path.display()
        );
    }

    let expected_png = std::fs::read(&golden_path)
        .unwrap_or_else(|e| panic!("read golden {}: {e}", golden_path.display()));
    if actual_png == expected_png {
        return;
    }

    // Mismatch: dump actual + diff and panic with a pointer.
    let actual_path = goldens_dir().join(format!("{}.actual.png", row.name));
    std::fs::write(&actual_path, &actual_png)
        .unwrap_or_else(|e| panic!("write actual {}: {e}", actual_path.display()));

    let actual_rgba = {
        let actual_file = actual_path.clone();
        decode_png_rgba(&actual_file)
    };
    let expected_rgba = decode_png_rgba(&golden_path);
    let diff_path = goldens_dir().join(format!("{}.diff.png", row.name));
    write_diff_mask(
        &diff_path,
        &actual_rgba,
        &expected_rgba,
        DISPLAY_WIDTH,
        DISPLAY_HEIGHT,
    );

    panic!(
        "{}: framebuffer doesn't match golden.\n  golden:  {}\n  actual:  {}\n  diff:    {}\nRe-run with EMU198X_UPDATE_GOLDENS=1 to accept the new output.",
        row.name,
        golden_path.display(),
        actual_path.display(),
        diff_path.display()
    );
}

#[test]
fn a500_ks13_no_disk() {
    run_row(&MATRIX[0]);
}

#[test]
fn a500_ks13_a501_no_disk() {
    run_row(&MATRIX[1]);
}

#[test]
fn a500_ks13_wb13() {
    run_row(&MATRIX[2]);
}

#[test]
fn a1000_ks12_no_disk() {
    run_row(&MATRIX[3]);
}
