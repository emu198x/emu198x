//! Boot golden screenshot for the Timex Sinclair 2068.
//!
//! Loads the TS2068 main ROM + EXROM, runs 240 frames at 60 Hz
//! NTSC, encodes the 704×296 hi-res-capable paletted framebuffer
//! through `SPECTRUM_PALETTE` as a 16-colour indexed PNG, and
//! either writes it as a new golden (when `UPDATE_GOLDENS=1` is
//! set or the golden is missing) or compares decoded bytes against
//! the checked-in golden in `tests/goldens/`.
//!
//! `#[ignore]`d because not every developer has the ROMs locally —
//! skipped (returning `ok`) when fixtures are missing.

use common_sinclair_zx_spectrum::palette::SPECTRUM_PALETTE;
use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH_HIRES};
use machine_timex_ts2068::{TimexModel, TimexTS2068};
use std::path::{Path, PathBuf};

fn rom_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".emu198x/roms/timex-ts2068"))
}

fn goldens_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/goldens")
}

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
    let mut encoder = png::Encoder::new(writer, SCREEN_WIDTH_HIRES as u32, SCREEN_HEIGHT as u32);
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
        (info.width as usize, info.height as usize),
        (SCREEN_WIDTH_HIRES, SCREEN_HEIGHT),
        "golden {} dimensions {}×{} don't match expected {}×{}",
        path.display(),
        info.width,
        info.height,
        SCREEN_WIDTH_HIRES,
        SCREEN_HEIGHT,
    );
    buf
}

fn compare_or_update(name: &str, framebuffer: &[u8]) {
    let path = goldens_dir().join(format!("{name}.png"));
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

#[test]
#[ignore = "requires local TS2068 ROMs at ~/.emu198x/roms/timex-ts2068/{ts2068,exrom}.rom"]
fn golden_ts2068_boot() {
    let Some(dir) = rom_dir() else {
        return;
    };
    let main_rom = dir.join("ts2068.rom");
    let exrom = dir.join("exrom.rom");
    if !main_rom.exists() || !exrom.exists() {
        eprintln!("TS2068 ROMs not found at {}", dir.display());
        return;
    }
    let mut machine = TimexTS2068::new(TimexModel::TS2068);
    machine
        .memory
        .load_rom(&main_rom)
        .expect("TS2068 main ROM should load");
    machine
        .memory
        .load_exrom(&exrom)
        .expect("TS2068 EXROM should load");

    // TS2068 boots into a diagnostic stripe pattern. On real
    // hardware that pattern resolves into the Timex/Sinclair menu
    // after a few seconds, but our implementation currently sits
    // on the stripes. The locked golden reflects that current
    // state honestly — when the boot path is extended to reach
    // BASIC, the golden refresh will catch it. 240 frames at 60 Hz
    // NTSC ≈ 4 s, more than enough to settle on the pattern.
    for _ in 0..240 {
        machine.run_frame();
    }
    compare_or_update("ts2068-boot", &machine.framebuffer);
}
