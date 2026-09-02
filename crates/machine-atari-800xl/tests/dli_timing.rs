//! When a register write made from a display-list interrupt reaches the
//! screen, driven through the machine.
//!
//! The program is `test-data/atari/dli-timing/atari-800xl-dli-timing.s`, a
//! cartridge this project wrote, so it needs no OS ROM. It draws six mode 2
//! text lines of glyph 0 from a font whose glyph 0 is solid, and a DLI on
//! the second line switches CHBASE to a font whose glyph 0 is empty; a DLI
//! on the fifth switches back. Zero page says how the interrupt makes the
//! write: with or without `STA WSYNC` first, and after how many padding
//! stores.
//!
//! ANTIC fetches a text line's glyph data during the line, not at its start
//! (Altirra Hardware Reference Manual, "Character mode playfield DMA": names
//! from cycle 18 at normal width and 26 at narrow, glyph data three cycles
//! later). A write that lands before that fetch shapes the line it lands on:
//!
//! - After `STA WSYNC` the CPU resumes at cycle 105, so a few four-cycle
//!   stores carry the CHBASE write past cycle 113 into the first cycles of
//!   the next line. That line, the first of the next text line, is drawn
//!   with the new font.
//! - Without WSYNC the write lands early in the interrupt's own line. On a
//!   narrow playfield that is before the first glyph fetch, so the last
//!   scan line of the interrupt's text line is drawn with the new font.

use std::path::PathBuf;

use machine_atari_800xl::{Atari800xl, Atari800xlRegion};

/// The framebuffer's first pixel sits on half colour clock 69 on NTSC, so a
/// colour clock `cc` lands at pixel `2 * cc - 69`.
const FIRST_HALF_CLOCK: usize = 69;

/// The display starts on scan line 8, which is framebuffer row 0; after 24
/// blank lines the six text lines cover framebuffer rows 24-71, eight each.
const FIRST_TEXT_ROW: usize = 24;
const TEXT_LINES: usize = 6;
const ROWS_PER_LINE: usize = 8;

/// DMACTL for each playfield width, and the colour clock its first
/// character starts on.
const NORMAL: (u8, usize) = (0x22, 48);
const NARROW: (u8, usize) = (0x21, 64);

/// Lit text is COLPF1's luminance on COLPF2's hue; the rest is COLPF2.
const LIT: u8 = 0x9E;
const EMPTY: u8 = 0x94;

fn cartridge() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-data/atari/dli-timing/atari-800xl-dli-timing.bin");
    std::fs::read(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

/// Boot the probe with the given DLI shape and return, for each scan line
/// of the text, whether every character on it is solid (`true`), empty
/// (`false`), or mixed (a panic: the line-oriented model never draws one).
fn font_rows(width: (u8, usize), wsync: bool, delay: u8) -> Vec<bool> {
    let mut machine = Atari800xl::new(None, None, Some(cartridge()), Atari800xlRegion::Ntsc, false)
        .expect("cartridge-only machine");
    machine.poke(0x80, width.0);
    machine.poke(0x81, u8::from(wsync));
    machine.poke(0x82, delay);
    // The program sets up during the first frame; the third is whole.
    for _ in 0..3 {
        machine.run_frame();
    }
    let lit = machine.gtia().colour_to_argb32(LIT);
    let empty = machine.gtia().colour_to_argb32(EMPTY);
    let fb_width = machine.framebuffer_width() as usize;
    let fb = machine.framebuffer();
    let chars = if width.0 == NARROW.0 { 32 } else { 40 };

    (0..TEXT_LINES * ROWS_PER_LINE)
        .map(|row| {
            let fb_row = FIRST_TEXT_ROW + row;
            let solid: Vec<bool> = (0..chars)
                .map(|ch| {
                    // Each character is four colour clocks; sample its second.
                    let x = 2 * (width.1 + 4 * ch + 1) - FIRST_HALF_CLOCK;
                    let pixel = fb[fb_row * fb_width + x];
                    assert!(
                        pixel == lit || pixel == empty,
                        "row {row} character {ch}: {pixel:08X} is neither text colour"
                    );
                    pixel == lit
                })
                .collect();
            assert!(
                solid.iter().all(|&s| s == solid[0]),
                "row {row} mixes fonts: {solid:?}"
            );
            solid[0]
        })
        .collect()
}

/// The expected font per scan line when the interrupt's write reaches the
/// screen `late` scan lines after the last line of the text line it fires
/// on: 0 means that line itself, 1 the first line of the next.
fn expected(late: usize) -> Vec<bool> {
    let switch_to_b = 2 * ROWS_PER_LINE - 1 + late;
    let switch_to_a = 5 * ROWS_PER_LINE - 1 + late;
    (0..TEXT_LINES * ROWS_PER_LINE)
        .map(|row| row < switch_to_b || row >= switch_to_a)
        .collect()
}

fn assert_rows(actual: &[bool], expected: &[bool], case: &str) {
    let differ: Vec<usize> = (0..actual.len())
        .filter(|&r| actual[r] != expected[r])
        .collect();
    assert!(
        differ.is_empty(),
        "{case}: rows {differ:?} drawn with the wrong font\n   got {actual:?}\nwanted {expected:?}"
    );
}

#[test]
fn a_write_spilling_past_wsync_shapes_the_line_it_lands_on() {
    // The interrupt comes back from WSYNC at cycle 105 and reaches its
    // stores at 110, so delay 0 writes at cycle 0 of the next line and each
    // further store moves the write four cycles on: 1-4 land at cycles
    // 4-16 of that line, before its first glyph fetch at cycle 21.
    for delay in 0..=4 {
        let rows = font_rows(NORMAL, true, delay);
        assert_rows(
            &rows,
            &expected(1),
            &format!("WSYNC, {delay} padding stores"),
        );
    }
}

#[test]
fn a_write_without_wsync_shapes_the_interrupt_line_on_a_narrow_playfield() {
    let rows = font_rows(NARROW, false, 0);
    assert_rows(&rows, &expected(0), "no WSYNC, narrow");
}
