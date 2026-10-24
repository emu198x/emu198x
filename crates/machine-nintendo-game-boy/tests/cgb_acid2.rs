//! cgb-acid2 (Matt Currie, MIT): the Game Boy Color PPU's headline
//! test. It draws a face that only comes out right when tile
//! attributes, VRAM banks, colour palettes, object priority by OAM
//! index and LCDC bit 0's master priority all work.
//!
//! The ROM and its reference picture are not committed. Point
//! `EMU198X_GB_CGB_ACID2_ROM` at `cgb-acid2.gbc` and
//! `EMU198X_GB_CGB_ACID2_REFERENCE` at `reference.png` (both from
//! <https://github.com/mattcurrie/cgb-acid2> releases), or put them in
//! `~/.emu198x/test-data/cgb-acid2/`. Without them the test records a
//! skip and passes.

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use machine_nintendo_game_boy::{BootProfile, GameBoy};

const WIDTH: usize = 160;
const HEIGHT: usize = 144;

fn fixture(var: &str, file: &str) -> Option<PathBuf> {
    let path = std::env::var_os(var).map(PathBuf::from).or_else(|| {
        std::env::var_os("HOME").map(|home| {
            PathBuf::from(home)
                .join(".emu198x/test-data/cgb-acid2")
                .join(file)
        })
    })?;
    path.exists().then_some(path)
}

/// The reference picture as 8-bit RGB triples.
fn reference_rgb(path: &PathBuf) -> Vec<[u8; 3]> {
    let file = std::fs::File::open(path).unwrap();
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!((info.width as usize, info.height as usize), (WIDTH, HEIGHT));
    let channels = info.color_type.samples();
    buf[..info.buffer_size()]
        .chunks_exact(channels)
        .map(|px| [px[0], px[1], px[2]])
        .collect()
}

/// RGB555 to 8-bit RGB the way cgb-acid2's reference was made: each
/// 5-bit channel scaled by `(c << 3) | (c >> 2)`, with no colour
/// correction.
fn rgb555_to_rgb(colour: u16) -> [u8; 3] {
    let scale = |c: u16| {
        let c = (c & 0x1F) as u8;
        (c << 3) | (c >> 2)
    };
    [scale(colour), scale(colour >> 5), scale(colour >> 10)]
}

#[test]
fn cgb_acid2_matches_reference() {
    let (Some(rom_path), Some(reference_path)) = (
        fixture("EMU198X_GB_CGB_ACID2_ROM", "cgb-acid2.gbc"),
        fixture("EMU198X_GB_CGB_ACID2_REFERENCE", "reference.png"),
    ) else {
        emu198x_test_skip::record(
            "skipping: set EMU198X_GB_CGB_ACID2_ROM and EMU198X_GB_CGB_ACID2_REFERENCE \
             (cgb-acid2.gbc and reference.png)",
        );
        return;
    };
    let rom = std::fs::read(rom_path).unwrap();
    let (_, mut gb) = GameBoy::from_rom_with_boot_profile(rom, BootProfile::Cgb).unwrap();
    assert!(gb.cgb_mode(), "cgb-acid2 is a CGB-only cartridge");

    // The test signals completion with `LD B,B`; a fixed budget of
    // frames is simpler and long enough.
    for _ in 0..60 {
        gb.run_frame();
    }

    let expected = reference_rgb(&reference_path);
    let picture = gb.color_framebuffer().unwrap();
    let mismatches: Vec<(usize, usize)> = picture
        .iter()
        .zip(&expected)
        .enumerate()
        .filter(|(_, (colour, want))| rgb555_to_rgb(**colour) != **want)
        .map(|(i, _)| (i % WIDTH, i / WIDTH))
        .collect();
    assert!(
        mismatches.is_empty(),
        "{} pixels differ from the reference; first at {:?}",
        mismatches.len(),
        &mismatches[..mismatches.len().min(8)]
    );
}
