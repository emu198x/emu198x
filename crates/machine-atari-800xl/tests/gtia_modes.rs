//! GTIA modes 9, 10 and 11, driven through the machine.
//!
//! The program is `test-data/atari/gtia-modes/atari-800xl-gtia-modes.s`, a
//! cartridge this project wrote, so it needs no OS ROM. It shows a mode F
//! screen whose pixel `p` carries nibble `p mod 16`, with PRIOR and COLBK
//! taken from zero page; each test picks a mode and says which colour every
//! pixel of a row should be.
//!
//! What each mode does with a nibble follows the hardware manual and both
//! reference emulators (Atari800 `antic.c` `draw_an_gtia*`, Altirra
//! `gtiarenderer.cpp` `RenderMode9/10/11`):
//!
//! - GTIA pairs ANTIC's output two colour clocks at a time, on even colour
//!   clocks, so a pixel is two colour clocks wide. Mode 10 pairs one clock
//!   later, which shifts its picture right by one colour clock.
//! - Mode 9 ORs the nibble into COLBK as luminance.
//! - Mode 10 picks a register: 0-3 COLPM0-3, 4-7 COLPF0-3, 8-11 COLBK,
//!   12-15 COLPF0-3 again.
//! - Mode 11 ORs the nibble into COLBK as hue, and shows nibble 0 at
//!   luminance 0 whatever COLBK's luminance is.

use std::path::PathBuf;

use machine_atari_800xl::{Atari800xl, Atari800xlRegion};

/// The framebuffer's first pixel sits on half colour clock 69 on NTSC, so a
/// colour clock `cc` lands at pixel `2 * cc - 69`.
const FIRST_HALF_CLOCK: i32 = 69;

/// A normal playfield is displayed from colour clock 48 to 207.
const PF_FIRST_CC: i32 = 48;
const PF_END_CC: i32 = 208;

/// The framebuffer shows colour clocks 35 to 218 either side of it.
const BORDER_FIRST_CC: i32 = 35;
const BORDER_END_CC: i32 = 219;

/// The display starts on scan line 8, which is framebuffer row 0; after 24
/// blank lines the mode F rows cover framebuffer rows 24-39.
const ROW: usize = 30;

/// The colour registers the cartridge programs.
const COLPM: [u8; 4] = [0x12, 0x24, 0x36, 0x48];
const COLPF: [u8; 4] = [0x5A, 0x6C, 0x7E, 0x92];

fn cartridge() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-data/atari/gtia-modes/atari-800xl-gtia-modes.bin");
    std::fs::read(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

/// Boot the probe in the given mode and return one row of the picture as
/// colour clocks: the ARGB pixel at each colour clock's first half.
fn row(prior: u8, colbk: u8) -> (Atari800xl, Vec<u32>) {
    let mut machine = Atari800xl::new(None, None, Some(cartridge()), Atari800xlRegion::Ntsc, false)
        .expect("cartridge-only machine");
    machine.poke(0x80, prior);
    machine.poke(0x81, colbk);
    // The program sets up during the first frame; the third is whole.
    for _ in 0..3 {
        machine.run_frame();
    }
    let width = machine.framebuffer_width() as usize;
    let fb = &machine.framebuffer()[ROW * width..(ROW + 1) * width];
    let clocks = (0..COLOUR_CLOCKS)
        .map(|cc| {
            let x = 2 * cc - FIRST_HALF_CLOCK;
            usize::try_from(x)
                .ok()
                .and_then(|x| fb.get(x).copied())
                .unwrap_or(0)
        })
        .collect();
    (machine, clocks)
}

const COLOUR_CLOCKS: i32 = 228;

/// Compare a row against the expected colour value per colour clock, naming
/// the first differences by colour clock.
fn assert_row(machine: &Atari800xl, actual: &[u32], expected: &[(i32, u8)]) {
    let diffs: Vec<String> = expected
        .iter()
        .filter_map(|&(cc, colour)| {
            let want = machine.gtia().colour_to_argb32(colour);
            let got = actual[cc as usize];
            (got != want)
                .then(|| format!("cc {cc}: got {got:08X}, want {want:08X} (${colour:02X})"))
        })
        .collect();
    assert!(
        diffs.is_empty(),
        "{} colour clocks differ; first: {}",
        diffs.len(),
        diffs.iter().take(6).cloned().collect::<Vec<_>>().join("; ")
    );
}

/// Pixel `p` of the playfield covers colour clocks `48 + 2p` and `49 + 2p`,
/// and carries nibble `p mod 16`. The border either side is nibble 0 too:
/// the modes transform everything ANTIC sends, and it sends 0 there.
fn playfield(colour_of: impl Fn(u8) -> u8) -> Vec<(i32, u8)> {
    (BORDER_FIRST_CC..BORDER_END_CC)
        .map(|cc| {
            let nibble = if (PF_FIRST_CC..PF_END_CC).contains(&cc) {
                (((cc - PF_FIRST_CC) / 2) % 16) as u8
            } else {
                0
            };
            (cc, colour_of(nibble))
        })
        .collect()
}

#[test]
fn mode_9_shows_sixteen_luminances_of_the_background_hue() {
    let (machine, actual) = row(0x40, 0x20);
    assert_row(&machine, &actual, &playfield(|nibble| 0x20 | nibble));
}

#[test]
fn mode_11_shows_sixteen_hues_at_the_background_luminance() {
    let (machine, actual) = row(0xC0, 0x06);
    assert_row(
        &machine,
        &actual,
        &playfield(|nibble| {
            if nibble == 0 {
                0x00
            } else {
                (nibble << 4) | 0x06
            }
        }),
    );
}

#[test]
fn mode_10_picks_a_colour_register_one_clock_late() {
    let (machine, actual) = row(0x80, 0x0E);
    let register = |nibble: u8| match nibble {
        0..=3 => COLPM[usize::from(nibble)],
        4..=7 | 12..=15 => COLPF[usize::from(nibble & 3)],
        _ => 0x0E,
    };
    // Each pixel lands one colour clock right of where modes 9 and 11 put
    // it. The playfield's first clock shows the pair GTIA formed from the
    // blank clock before it and the first nibble's top two bits — 0, so
    // COLPM0 like the border — and the last pixel spills one clock into
    // the right border.
    let mut expected: Vec<(i32, u8)> = (BORDER_FIRST_CC..=PF_FIRST_CC)
        .map(|cc| (cc, COLPM[0]))
        .collect();
    expected.extend((PF_FIRST_CC + 1..=PF_END_CC).map(|cc| {
        let nibble = (((cc - PF_FIRST_CC - 1) / 2) % 16) as u8;
        (cc, register(nibble))
    }));
    expected.extend((PF_END_CC + 1..BORDER_END_CC).map(|cc| (cc, COLPM[0])));
    assert_row(&machine, &actual, &expected);
}

#[test]
fn plain_mode_f_is_untouched() {
    // PRIOR 0: hi-res pixels, COLPF2 background with COLPF1's luminance on
    // lit bits. Nibble 0 is four unlit pixels, nibble 15 four lit ones.
    let (machine, actual) = row(0x00, 0x0E);
    let expected: Vec<(i32, u8)> = [(0, 0x7E), (15, 0x7C)]
        .iter()
        .map(|&(nibble, colour)| (PF_FIRST_CC + 2 * nibble, colour))
        .collect();
    assert_row(&machine, &actual, &expected);
}
