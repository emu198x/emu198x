//! Golden-screenshot tests for the eight in-scope Spectrum variants.
//!
//! Each test boots its variant from real ROMs under
//! `~/.emu198x/roms/<system>/`, runs a fixed number of frames, expands
//! the paletted framebuffer through `SPECTRUM_PALETTE`, encodes a
//! 16-colour indexed PNG, and either writes it as a new golden (when
//! `UPDATE_GOLDENS=1` is set or the golden is missing) or compares
//! decoded bytes against the existing golden.
//!
//! ## Workflow
//!
//! First run:
//! ```sh
//! UPDATE_GOLDENS=1 cargo test -p runtime-sinclair-zx-spectrum \
//!     --test goldens -- --include-ignored
//! ```
//! Eyeball the eight PNGs in `tests/goldens/`, commit them.
//!
//! Subsequent runs:
//! ```sh
//! cargo test -p runtime-sinclair-zx-spectrum --test goldens \
//!     -- --include-ignored
//! ```
//! Each test reads its golden, decodes to indexed bytes, and compares
//! to the live framebuffer. A mismatch fails with a reproducible diff
//! count (no image-diff library — just byte-equal vs not).
//!
//! All tests are `#[ignore]`d so CI without ROMs stays green; the
//! Code198x curriculum pipeline is the consumer that exercises them.

use common_sinclair_zx_spectrum::SPECTRUM_PALETTE;
use machine_sinclair_zx_spectrum_16k::Spectrum16K;
use machine_sinclair_zx_spectrum_48k::Spectrum48k;
use machine_sinclair_zx_spectrum_128k::Spectrum128K;
use machine_sinclair_zx_spectrum_plus::SpectrumPlus;
use machine_sinclair_zx_spectrum_plus2::SpectrumPlus2;
use machine_sinclair_zx_spectrum_plus2a::SpectrumPlus2A;
use machine_sinclair_zx_spectrum_plus2b::SpectrumPlus2B;
use machine_sinclair_zx_spectrum_plus3::SpectrumPlus3;
use std::path::{Path, PathBuf};

const FRAME_WIDTH: u32 = 352;
const FRAME_HEIGHT: u32 = 296;

fn rom_root() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".emu198x/roms"))
}

fn goldens_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/goldens")
}

/// Spectrum palette flattened to RGB bytes (PNG indexed colour mode
/// stores RGB triplets, alpha goes in a separate `tRNS` chunk we
/// don't need — the Spectrum has no transparency).
fn palette_rgb() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(SPECTRUM_PALETTE.len() * 3);
    for entry in &SPECTRUM_PALETTE {
        let r = ((entry >> 24) & 0xFF) as u8;
        let g = ((entry >> 16) & 0xFF) as u8;
        let b = ((entry >> 8) & 0xFF) as u8;
        bytes.extend_from_slice(&[r, g, b]);
    }
    bytes
}

fn write_png(path: &Path, framebuffer: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create goldens dir");
    }
    let file = std::fs::File::create(path).expect("create golden file");
    let writer = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, FRAME_WIDTH, FRAME_HEIGHT);
    encoder.set_color(png::ColorType::Indexed);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_palette(palette_rgb());
    let mut writer = encoder.write_header().expect("write png header");
    writer
        .write_image_data(framebuffer)
        .expect("write png image data");
}

fn read_png_indexed(path: &Path) -> Vec<u8> {
    let file = std::fs::File::open(path).expect("open golden");
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let mut reader = decoder.read_info().expect("decode png header");
    let mut buf = vec![
        0u8;
        reader
            .output_buffer_size()
            .expect("png buffer size fits in usize")
    ];
    let info = reader.next_frame(&mut buf).expect("decode png frame");
    buf.truncate(info.buffer_size());
    assert_eq!(
        info.color_type,
        png::ColorType::Indexed,
        "golden {} should be indexed PNG",
        path.display()
    );
    assert_eq!(
        (info.width, info.height),
        (FRAME_WIDTH, FRAME_HEIGHT),
        "golden {} dimensions {}×{} don't match expected {}×{}",
        path.display(),
        info.width,
        info.height,
        FRAME_WIDTH,
        FRAME_HEIGHT,
    );
    buf
}

fn compare_or_update(name: &str, framebuffer: &[u8]) {
    let path = goldens_dir().join(format!("{name}-boot.png"));
    let updating = std::env::var_os("UPDATE_GOLDENS").is_some();
    let missing = !path.exists();

    if updating || missing {
        write_png(&path, framebuffer);
        if missing && !updating {
            panic!(
                "golden {} did not exist — wrote it now, re-run to verify",
                path.display()
            );
        }
        return;
    }

    let expected = read_png_indexed(&path);
    if expected == framebuffer {
        return;
    }
    let differing = expected
        .iter()
        .zip(framebuffer.iter())
        .filter(|(a, b)| a != b)
        .count();
    panic!(
        "framebuffer for {name} differs from {} ({differing} of {} pixels). \
         Re-run with UPDATE_GOLDENS=1 to refresh after eyeballing the change.",
        path.display(),
        framebuffer.len(),
    );
}

fn rom_dir(system: &str) -> Option<PathBuf> {
    rom_root().map(|r| r.join(system))
}

fn skip_if_missing(path: &Path) -> bool {
    if !path.exists() {
        eprintln!("ROM not found at {} — skipping", path.display());
        return true;
    }
    false
}

fn run_frames<F: FnMut()>(count: u32, mut step: F) {
    for _ in 0..count {
        step();
    }
}

// ─────────────────────────────────────────────────────────────────────
// Tests — one per in-scope variant. Each returns early if its ROMs
// aren't present; assertion is via compare_or_update.
// ─────────────────────────────────────────────────────────────────────

#[test]
#[ignore = "requires local 48K ROM at ~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom"]
fn golden_spectrum_16k_boot() {
    let Some(dir) = rom_dir("sinclair-zx-spectrum-48k") else {
        emu198x_test_skip::skip!("Spectrum ROMs not staged (~/.emu198x/roms/sinclair-zx-spectrum)");
    };
    let rom = dir.join("48.rom");
    if skip_if_missing(&rom) {
        return;
    }
    let bytes = std::fs::read(&rom).expect("read 48K ROM");
    let mut machine = Spectrum16K::new();
    machine.load_rom_bytes(&bytes).expect("load 16 KiB ROM");
    run_frames(200, || machine.run_frame());
    compare_or_update("spectrum-16k", machine.framebuffer());
}

#[test]
#[ignore = "requires local 48K ROM at ~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom"]
fn golden_spectrum_48k_boot() {
    let Some(dir) = rom_dir("sinclair-zx-spectrum-48k") else {
        emu198x_test_skip::skip!("Spectrum ROMs not staged (~/.emu198x/roms/sinclair-zx-spectrum)");
    };
    let rom = dir.join("48.rom");
    if skip_if_missing(&rom) {
        return;
    }
    let bytes = std::fs::read(&rom).expect("read 48K ROM");
    let mut machine = Spectrum48k::new();
    machine.load_rom_bytes(&bytes).expect("load 16 KiB ROM");
    run_frames(200, || machine.run_frame());
    compare_or_update("spectrum-48k", machine.framebuffer());
}

#[test]
#[ignore = "requires local 48K ROM at ~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom"]
fn golden_spectrum_plus_boot() {
    let Some(dir) = rom_dir("sinclair-zx-spectrum-48k") else {
        emu198x_test_skip::skip!("Spectrum ROMs not staged (~/.emu198x/roms/sinclair-zx-spectrum)");
    };
    let rom = dir.join("48.rom");
    if skip_if_missing(&rom) {
        return;
    }
    let bytes = std::fs::read(&rom).expect("read 48K ROM");
    let mut machine = SpectrumPlus::new();
    machine.load_rom_bytes(&bytes).expect("load 16 KiB ROM");
    run_frames(200, || machine.run_frame());
    compare_or_update("spectrum-plus", machine.framebuffer());
}

#[test]
#[ignore = "requires local 128K ROMs at ~/.emu198x/roms/sinclair-zx-spectrum-128k/{128-0,128-1}.rom"]
fn golden_spectrum_128k_boot() {
    let Some(dir) = rom_dir("sinclair-zx-spectrum-128k") else {
        emu198x_test_skip::skip!("Spectrum ROMs not staged (~/.emu198x/roms/sinclair-zx-spectrum)");
    };
    let rom0 = dir.join("128-0.rom");
    let rom1 = dir.join("128-1.rom");
    if skip_if_missing(&rom0) || skip_if_missing(&rom1) {
        return;
    }
    let mut machine = Spectrum128K::new();
    machine.memory.load_rom0(&rom0).expect("load ROM 0");
    machine.memory.load_rom1(&rom1).expect("load ROM 1");
    run_frames(200, || machine.run_frame());
    compare_or_update("spectrum-128k", &machine.framebuffer);
}

#[test]
#[ignore = "requires local +2 ROMs at ~/.emu198x/roms/amstrad-zx-spectrum-plus2/{plus2-0,plus2-1}.rom"]
fn golden_spectrum_plus2_boot() {
    let Some(dir) = rom_dir("amstrad-zx-spectrum-plus2") else {
        emu198x_test_skip::skip!("Spectrum ROMs not staged (~/.emu198x/roms/sinclair-zx-spectrum)");
    };
    let rom0 = dir.join("plus2-0.rom");
    let rom1 = dir.join("plus2-1.rom");
    if skip_if_missing(&rom0) || skip_if_missing(&rom1) {
        return;
    }
    let mut machine = SpectrumPlus2::new();
    machine.memory.load_rom0(&rom0).expect("load ROM 0");
    machine.memory.load_rom1(&rom1).expect("load ROM 1");
    run_frames(200, || machine.run_frame());
    compare_or_update("spectrum-plus2", &machine.framebuffer);
}

fn load_plus3_roms<V: common_sinclair_zx_spectrum_amstrad_class::AmstradVariant>(
    machine: &mut common_sinclair_zx_spectrum_amstrad_class::SpectrumAmstradClassCore<V>,
    dir: &Path,
) -> bool {
    for i in 0..4 {
        let rom = dir.join(format!("plus3-{i}.rom"));
        if skip_if_missing(&rom) {
            return false;
        }
        machine
            .memory
            .load_rom(i, &rom)
            .unwrap_or_else(|e| panic!("ROM {i} should load: {e}"));
    }
    true
}

#[test]
#[ignore = "requires local +3 ROMs at ~/.emu198x/roms/amstrad-zx-spectrum-plus3/{plus3-0..3}.rom"]
fn golden_spectrum_plus2a_boot() {
    let Some(dir) = rom_dir("amstrad-zx-spectrum-plus3") else {
        emu198x_test_skip::skip!("Spectrum ROMs not staged (~/.emu198x/roms/sinclair-zx-spectrum)");
    };
    let mut machine = SpectrumPlus2A::new();
    if !load_plus3_roms(&mut machine, &dir) {
        return;
    }
    run_frames(250, || machine.run_frame());
    compare_or_update("spectrum-plus2a", &machine.framebuffer);
}

#[test]
#[ignore = "requires local +2B ROMs at ~/.emu198x/roms/amstrad-zx-spectrum-plus2b/{plus3-0..3}.rom"]
fn golden_spectrum_plus2b_boot() {
    // +2B shipped with ROM v4.1 (the v4.0 set lives in `plus3/` and is
    // shared by +2A and early +3s). The split is deliberate so the
    // +2B golden reflects its own "+2B" menu header rather than
    // showing "+2A" via shared ROMs.
    let Some(dir) = rom_dir("amstrad-zx-spectrum-plus2b") else {
        emu198x_test_skip::skip!("Spectrum ROMs not staged (~/.emu198x/roms/sinclair-zx-spectrum)");
    };
    let mut machine = SpectrumPlus2B::new();
    if !load_plus3_roms(&mut machine, &dir) {
        return;
    }
    run_frames(250, || machine.run_frame());
    compare_or_update("spectrum-plus2b", &machine.framebuffer);
}

#[test]
#[ignore = "requires local +3 ROMs at ~/.emu198x/roms/amstrad-zx-spectrum-plus3/{plus3-0..3}.rom"]
fn golden_spectrum_plus3_boot() {
    let Some(dir) = rom_dir("amstrad-zx-spectrum-plus3") else {
        emu198x_test_skip::skip!("Spectrum ROMs not staged (~/.emu198x/roms/sinclair-zx-spectrum)");
    };
    let mut machine = SpectrumPlus3::new();
    if !load_plus3_roms(&mut machine, &dir) {
        return;
    }
    run_frames(250, || machine.run_frame());
    compare_or_update("spectrum-plus3", &machine.framebuffer);
}
