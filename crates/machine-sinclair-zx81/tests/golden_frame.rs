//! ZX81 boot-screen golden frame.
//!
//! A regression net for the display model. The machine boots its ROM, settles,
//! and its framebuffer is byte-compared against a committed PNG.
//!
//! # What this is, and is not
//!
//! The committed image is **Emu198x output**, so per
//! `knowledge/processes/golden-image-capture.md` it is a regression baseline
//! and not an independent accuracy reference. It cannot tell you the picture
//! is *right*; it tells you the picture *changed*, which is the thing nothing
//! previously did.
//!
//! That gap is worth having anyway. The display fetch formed its pattern
//! address in a way that disagreed with the hardware manual for any character
//! set other than the ROM's own, and nothing caught it — because nothing
//! compared the output to anything at all. A baseline turns that class of
//! change from invisible into a failing test.
//!
//! **It is now corroborated.** `mame_cross_check.rs` compares the 256x192
//! text area against a MAME 0.289 capture and finds it pixel-identical, with
//! the two rasters aligned by the field line the text starts on. That makes
//! this an accuracy reference in the sense
//! `knowledge/processes/golden-image-capture.md` means — provenance recording
//! an external capture — on the same footing as the Amiga boot goldens, which
//! are checked against FS-UAE.
//!
//! Two honest limits. MAME is an emulator, not a machine, so the claim is that
//! two independent implementations agree rather than that either matches
//! hardware. And the agreement is vertical: horizontally MAME places the ZX80
//! and ZX81 pictures 26 pixels apart where we place both in the same column,
//! which is #1123.
//!
//! # Updating
//!
//! `EMU198X_UPDATE_GOLDENS=1` rewrites the baseline. Only for a reviewed
//! change: establish the cause first, then look at the whole image, not just
//! the diff. On mismatch the harness writes `.actual.png` and `.diff.png`
//! beside the golden.

use std::{env, fs, io::BufReader, path::PathBuf};

use machine_sinclair_zx81::Zx81;

/// Frames to settle before capture. The ROM has established its display and
/// the `K` cursor well before this.
const SETTLE_FRAMES: usize = 400;

fn rom() -> Option<Vec<u8>> {
    let path = env::var("EMU198X_ZX81_ROM").ok().or_else(|| {
        env::var("HOME")
            .ok()
            .map(|h| format!("{h}/.emu198x/roms/sinclair-zx81/zx81.rom"))
    })?;
    fs::read(path).ok()
}

fn goldens_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("goldens")
}

/// ARGB framebuffer to the RGB bytes a PNG holds.
fn to_rgb(pixels: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixels.len() * 3);
    for p in pixels {
        out.push((p >> 16) as u8);
        out.push((p >> 8) as u8);
        out.push(*p as u8);
    }
    out
}

fn write_png(path: &PathBuf, rgb: &[u8], width: u32, height: u32) {
    let file = fs::File::create(path).expect("create PNG");
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .expect("PNG header")
        .write_image_data(rgb)
        .expect("PNG data");
}

fn read_png(path: &PathBuf) -> Option<(Vec<u8>, u32, u32)> {
    let decoder = png::Decoder::new(BufReader::new(fs::File::open(path).ok()?));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    Some((buf, info.width, info.height))
}

#[test]
fn the_boot_screen_matches_its_baseline() {
    let Some(rom) = rom() else {
        emu198x_test_skip::skip!(
            "ZX81 ROM not staged — set EMU198X_ZX81_ROM or place zx81.rom at ~/.emu198x/roms/sinclair-zx81/"
        );
    };

    let mut machine = Zx81::new(rom, 16384).expect("machine");
    for _ in 0..SETTLE_FRAMES {
        machine.run_frame();
    }

    let width = machine.framebuffer_width();
    let height = machine.framebuffer_height();
    let actual = to_rgb(machine.framebuffer());

    let golden = goldens_dir().join("zx81-boot.png");
    if env::var("EMU198X_UPDATE_GOLDENS").is_ok() {
        fs::create_dir_all(goldens_dir()).expect("goldens dir");
        write_png(&golden, &actual, width, height);
        eprintln!("wrote {}", golden.display());
        return;
    }

    let Some((expected, gw, gh)) = read_png(&golden) else {
        panic!(
            "no baseline at {} — run once with EMU198X_UPDATE_GOLDENS=1",
            golden.display()
        );
    };
    assert_eq!(
        (gw, gh),
        (width, height),
        "baseline is {gw}x{gh} but the machine renders {width}x{height}; \
         a dimension change is a display-model change and needs review, \
         not a silent rescale",
    );

    if actual != expected {
        let differing = actual
            .as_chunks::<3>()
            .0
            .iter()
            .zip(expected.as_chunks::<3>().0.iter())
            .filter(|(a, b)| a != b)
            .count();
        write_png(
            &goldens_dir().join("zx81-boot.actual.png"),
            &actual,
            width,
            height,
        );
        let diff: Vec<u8> = actual
            .as_chunks::<3>()
            .0
            .iter()
            .zip(expected.as_chunks::<3>().0.iter())
            .flat_map(|(a, b)| if a == b { [0, 0, 0] } else { [255, 0, 0] })
            .collect();
        write_png(
            &goldens_dir().join("zx81-boot.diff.png"),
            &diff,
            width,
            height,
        );
        panic!(
            "{differing} of {} pixels differ from the baseline; \
             wrote zx81-boot.actual.png and zx81-boot.diff.png beside it",
            width * height,
        );
    }
}
