//! Amiga boot-path golden matrix.
//!
//! A table-driven regression net for Amiga boot screens. Each row is
//! a `(Model, Kickstart ROM, optional ADF, settle frame count)`
//! combination; the harness runs the machine for the settle frames,
//! captures Denise's framebuffer, crops to the matrix's historical
//! FS-UAE-sized PAL region, and byte-compares against a regression PNG
//! on disk.
//!
//! # Framing
//!
//! Our emulator renders at **768×576** — full PAL Standard overscan,
//! the same region the runtime shows users. FS-UAE's default PAL
//! output crops 8 px each side horizontally and 2 scan-lines top and
//! bottom to **752×572**. Goldens retain that dimension and the harness
//! applies the same symmetric crop to our 768×576 output before
//! comparison.
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
//! The current committed PNGs are Emu198x-produced regression baselines,
//! not independent FS-UAE accuracy references. See
//! `knowledge/processes/golden-image-capture.md` for the provenance rule.
//! `EMU198X_UPDATE_GOLDENS=1` is available only for a reviewed baseline
//! change: establish the cause, compare the changed region with independent
//! evidence, and inspect the complete image before retaining it.
//!
//! On mismatch the harness writes two debug PNGs next to the
//! golden:
//!
//! - `<name>.actual.png` — the frame the emulator produced (cropped)
//! - `<name>.diff.png`   — a pixel mask highlighting differences
//!
//! Both are gitignored so they don't pollute the tree.
//!
//! # Scope
//!
//! OCS — A1000 bootstrap + A500 / A500+A501 with Kickstart-era boot
//! paths, with and without a Workbench ADF inserted. Plus one AGA row:
//! A1200 + Kickstart 3.1 booting Workbench 3.1 to the desktop (issue
//! #42), which locks the FMODE wide-fetch and 68020 EA-decode paths.
//! Remaining phases extend the matrix to ECS (A500+/A600), more AGA
//! (A4000/CD32), and HDD boot.

use std::io::BufReader;
use std::path::{Path, PathBuf};

use emu198x_shell::{
    HeadlessScript, HeadlessSession, MediaKind, ScriptMediaKind, ScriptStep, read_media_asset,
};
use format_commodore_amiga_adf::Adf;
use runtime_commodore_amiga::{
    A500_PAL_FRAME_TICKS, AmigaA1200Runtime, AmigaOcsRuntime, AmigaSessionQueryProvider,
    DISPLAY_HEIGHT, DISPLAY_WIDTH, Model,
};

/// One row in the golden matrix.
struct GoldenRow {
    /// Short kebab-case name. Also the PNG filename stem.
    name: &'static str,
    /// Amiga model (carries RAM layout + profile metadata).
    model: Model,
    /// Kickstart ROM filename under `~/.emu198x/roms/commodore-amiga/`.
    kickstart: &'static str,
    /// Boot flow to execute before capturing the frame.
    boot: BootFlow,
}

#[derive(Clone, Copy)]
enum DiskAsset {
    HomeMedia(&'static str),
    A1000KickstartZip,
}

#[derive(Clone, Copy)]
enum BootFlow {
    Direct {
        /// Optional disk image to insert before ticking.
        disk: Option<DiskAsset>,
        /// PAL frames to tick before capture.
        settle_frames: u64,
    },
    A1000KickstartSwap {
        /// Kickstart disk image loaded into DF0 first.
        kickstart_disk: DiskAsset,
        /// Workbench disk swapped into DF0 after WOM lock.
        workbench_disk: DiskAsset,
        /// PAL frames to run after the Workbench disk swap.
        post_swap_frames: u64,
    },
}

/// FS-UAE's default PAL crop — 8 px each side horizontally, 2
/// scan-lines top and bottom of our 768×576 PAL Standard frame.
const FSUAE_W: u32 = 752;
const FSUAE_H: u32 = 572;

/// Historical settle-frame count retained by the current regression
/// baselines. The Kickstart 1.2 / 1.3 insert-disk rows capture frame 250.
const KS13_SETTLE_FRAMES: u64 = 250;

const MATRIX: &[GoldenRow] = &[
    GoldenRow {
        name: "a500-ks13-no-disk",
        model: Model::A500OcsPal,
        kickstart: "kick13.rom",
        boot: BootFlow::Direct {
            disk: None,
            settle_frames: KS13_SETTLE_FRAMES,
        },
    },
    GoldenRow {
        name: "a500-ks13-a501-no-disk",
        model: Model::A500OcsPalA501,
        kickstart: "kick13.rom",
        boot: BootFlow::Direct {
            disk: None,
            settle_frames: KS13_SETTLE_FRAMES,
        },
    },
    GoldenRow {
        name: "a500-ks13-wb13",
        model: Model::A500OcsPalA501,
        kickstart: "kick13.rom",
        boot: BootFlow::Direct {
            disk: Some(DiskAsset::HomeMedia("workbench-1.3.adf")),
            // The corrected 112-CCK normal MFM stream reaches this reviewed
            // desktop after frame 3000. Frame 3500 retains a measured margin
            // and remains pixel-exact with the existing regression baseline.
            settle_frames: 3500,
        },
    },
    GoldenRow {
        name: "a1000-ks12-no-disk",
        model: Model::A1000OcsPal,
        kickstart: "a1000-bootstrap.rom",
        boot: BootFlow::Direct {
            disk: None,
            settle_frames: KS13_SETTLE_FRAMES,
        },
    },
    GoldenRow {
        name: "a1000-ks12-wb12",
        model: Model::A1000OcsPal,
        kickstart: "a1000-bootstrap.rom",
        boot: BootFlow::A1000KickstartSwap {
            kickstart_disk: DiskAsset::A1000KickstartZip,
            workbench_disk: DiskAsset::HomeMedia("workbench-1.2.adf"),
            post_swap_frames: 3000,
        },
    },
    // AGA: A1200 + Kickstart 3.1 booting Workbench 3.1 to the desktop.
    // Locks the FMODE bitplane wide-fetch and 68020 full-format EA decode
    // paths against regression (issue #42). The bootable disk is "Disk 2
    // (Workbench)" of the WB 3.1 six-disk set.
    GoldenRow {
        name: "a1200-ks31-wb31",
        model: Model::A1200AgaPal,
        kickstart: "kick31a1200.rom",
        boot: BootFlow::Direct {
            disk: Some(DiskAsset::HomeMedia(
                "wb31/Workbench v3.1 rev 40.42 (1996)(ESCOM)(M10)(Disk 2 of 6)(Workbench).adf",
            )),
            settle_frames: 1800,
        },
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

fn a1000_kickstart_disk_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("EMU198X_AMIGA_A1000_KICKSTART_DISK") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate dir should have repo-root parents");
    let sibling_archive = repo_root
        .parent()
        .expect("repo root should have a parent")
        .join("Emu198x-docs-archive-2026-04-19/Reference/amiga/Kickstart-Disks/Kickstart-Disk v1.2 r33.180 (1986)(Commodore)(A1000).zip");
    if sibling_archive.exists() {
        return Some(sibling_archive);
    }

    None
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
        eprintln!("skipping {row}: {kind} missing at {}", path.display());
        return None;
    }
    Some(std::fs::read(path).unwrap_or_else(|e| panic!("read {kind} at {}: {e}", path.display())))
}

fn disk_asset_path(spec: DiskAsset, row: &str) -> Option<PathBuf> {
    match spec {
        DiskAsset::HomeMedia(name) => {
            let Some(media) = media_dir() else {
                eprintln!("skipping {row}: $HOME not set");
                return None;
            };
            Some(media.join(name))
        }
        DiskAsset::A1000KickstartZip => {
            let Some(path) = a1000_kickstart_disk_path() else {
                eprintln!(
                    "skipping {row}: A1000 Kickstart disk missing; set EMU198X_AMIGA_A1000_KICKSTART_DISK"
                );
                return None;
            };
            Some(path)
        }
    }
}

fn load_optional_disk_asset(spec: DiskAsset, row: &str) -> Option<Vec<u8>> {
    let path = disk_asset_path(spec, row)?;
    if !path.exists() {
        eprintln!("skipping {row}: disk image missing at {}", path.display());
        return None;
    }
    Some(
        read_media_asset(&path, MediaKind::Disk)
            .unwrap_or_else(|e| panic!("{row}: load disk {}: {e}", path.display()))
            .bytes,
    )
}

/// Tick the runtime for `frames` PAL frames. Uses the machine's
/// `tick()` directly rather than `run_until` so we bypass the
/// frame-sink plumbing — the test only cares about the final
/// framebuffer, not per-frame emission.
fn tick_frames(rt: &mut AmigaOcsRuntime, frames: u64) {
    for _ in 0..(frames * A500_PAL_FRAME_TICKS) {
        rt.machine_mut().tick();
    }
}

/// Capture the Denise framebuffer, crop to the matrix's historical
/// FS-UAE-sized PAL region, and return the cropped bytes as RGB.
///
/// The crop is a centered `(DISPLAY_WIDTH - FSUAE_W) / 2`-pixel
/// horizontal trim and `(DISPLAY_HEIGHT - FSUAE_H) / 2`-scanline
/// vertical trim, discarding the outer PAL overscan border that
/// FS-UAE doesn't show.
fn capture_fsuae_rgb(rt: &AmigaOcsRuntime) -> Vec<u8> {
    crop_fsuae_rgb(rt.machine().denise().framebuffer())
}

/// Crop a full 768×576 PAL Standard framebuffer to FS-UAE's default
/// PAL region and return it as RGB bytes. Chipset-agnostic — OCS, ECS
/// and AGA all render to the same `DISPLAY_WIDTH × DISPLAY_HEIGHT`
/// buffer, so the same crop applies.
fn crop_fsuae_rgb(fb: &[u32]) -> Vec<u8> {
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

/// Encode RGB bytes (no alpha) in the regression baseline's pixel format.
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
fn write_diff_mask(path: &Path, actual_rgb: &[u8], expected_rgb: &[u8], w: u32, h: u32) {
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
    // png 0.18 requires the underlying reader to implement BufRead, and
    // `output_buffer_size()` now returns Option<usize> (None if the
    // computed size overflows usize).
    let file = std::fs::File::open(path).expect("open golden");
    let decoder = png::Decoder::new(BufReader::new(file));
    let mut reader = decoder.read_info().expect("png read info");
    let mut buf = vec![
        0;
        reader
            .output_buffer_size()
            .expect("png buffer size fits in usize")
    ];
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
    let Some(rom_bytes) = load_optional_artifact(&rom_path, "Kickstart ROM", row.name) else {
        return;
    };

    let actual_rgb = match row.boot {
        BootFlow::Direct {
            disk,
            settle_frames,
        } if row.model.is_aga() => {
            // AGA (A1200) boots through the Lisa chip stack, so it needs
            // the AGA runtime rather than the OCS one. The boot shape is
            // otherwise identical to the OCS Direct path: optional disk,
            // tick to settle, crop the same 768×576 framebuffer.
            let mut rt = AmigaA1200Runtime::new(row.model, rom_bytes)
                .unwrap_or_else(|e| panic!("{}: build runtime: {e:?}", row.name));
            if let Some(spec) = disk {
                let Some(bytes) = load_optional_disk_asset(spec, row.name) else {
                    return;
                };
                let adf = Adf::from_bytes(bytes)
                    .unwrap_or_else(|e| panic!("{}: decode ADF: {e}", row.name));
                rt.machine_mut().insert_adf(adf);
            }
            for _ in 0..(settle_frames * A500_PAL_FRAME_TICKS) {
                rt.machine_mut().tick();
            }
            crop_fsuae_rgb(rt.machine().denise().framebuffer())
        }
        BootFlow::Direct {
            disk,
            settle_frames,
        } => {
            let mut rt = AmigaOcsRuntime::new(row.model, rom_bytes)
                .unwrap_or_else(|e| panic!("{}: build runtime: {e:?}", row.name));
            if let Some(spec) = disk {
                let Some(bytes) = load_optional_disk_asset(spec, row.name) else {
                    return;
                };
                let adf = Adf::from_bytes(bytes)
                    .unwrap_or_else(|e| panic!("{}: decode ADF: {e}", row.name));
                rt.machine_mut().insert_adf(adf);
            }
            tick_frames(&mut rt, settle_frames);
            capture_fsuae_rgb(&rt)
        }
        BootFlow::A1000KickstartSwap {
            kickstart_disk,
            workbench_disk,
            post_swap_frames,
        } => {
            let Some(kickstart_path) = disk_asset_path(kickstart_disk, row.name) else {
                return;
            };
            let Some(workbench_path) = disk_asset_path(workbench_disk, row.name) else {
                return;
            };
            let runtime = AmigaOcsRuntime::new(row.model, rom_bytes)
                .unwrap_or_else(|e| panic!("{}: build runtime: {e:?}", row.name));
            let mut session = HeadlessSession::new_with_query_provider(
                runtime,
                A500_PAL_FRAME_TICKS,
                AmigaSessionQueryProvider,
            );
            let script = HeadlessScript {
                steps: vec![
                    ScriptStep::LoadMedia {
                        slot: "floppy-0".to_owned(),
                        kind: ScriptMediaKind::Disk,
                        path: kickstart_path,
                        writable: false,
                    },
                    ScriptStep::WaitForQueryBool {
                        path: "a1000.wom_locked".to_owned(),
                        value: true,
                        max_frames: 1800,
                    },
                    ScriptStep::WaitForQueryBool {
                        path: "disk.motor_spinning".to_owned(),
                        value: false,
                        max_frames: 600,
                    },
                    ScriptStep::LoadMedia {
                        slot: "floppy-0".to_owned(),
                        kind: ScriptMediaKind::Disk,
                        path: workbench_path,
                        writable: false,
                    },
                    ScriptStep::RunFrames {
                        frames: post_swap_frames
                            .try_into()
                            .expect("post-swap frame count should fit in u32"),
                    },
                ],
            };
            script
                .execute_collect(&mut session)
                .unwrap_or_else(|e| panic!("{}: execute scripted boot flow: {e}", row.name));
            capture_fsuae_rgb(session.machine())
        }
    };

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
        // First-run safety: a missing reviewed regression baseline must not
        // be invented silently.
        panic!(
            "{}: golden missing at {}. \
             See knowledge/processes/golden-image-capture.md.",
            row.name,
            golden_path.display()
        );
    }

    let (expected_rgb, gw, gh) = decode_png_rgb(&golden_path);
    if gw != FSUAE_W || gh != FSUAE_H {
        panic!(
            "{}: golden dimensions {}×{} do not match matrix geometry {}×{} at {}",
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
    std::fs::write(&actual_path, encode_rgb_png(&actual_rgb, FSUAE_W, FSUAE_H))
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

#[test]
fn a1000_ks12_wb12() {
    run_row(&MATRIX[4]);
}

#[test]
fn a1200_ks31_wb31() {
    run_row(&MATRIX[5]);
}
