//! Player/missile DMA, driven through the machine.
//!
//! A player reaches the screen through a chain no chip test covers: ANTIC
//! fetches its bitmap over DMA, the machine hands the bytes to GTIA, GTIA
//! decides whether to take them and where to put them. `atari-antic` and
//! `atari-gtia` each pin their own end with register writes; this runs the
//! whole chain from a program.
//!
//! The program is `test-data/atari/pm-dma/atari-800xl-pm-dma.s`, a cartridge
//! this project wrote, so it needs no OS ROM: the 800XL starts a cartridge
//! directly when it has none. The cartridge takes DMACTL, GRACTL and VDELAY
//! from zero page, and paints a playfield that is background everywhere, so
//! every lit pixel is an object and each test is one register triple and
//! the set of pixels it should light.

use std::collections::BTreeSet;
use std::path::PathBuf;

use machine_atari_800xl::{Atari800xl, Atari800xlRegion};

/// DMACTL: normal playfield, display list DMA, player and missile DMA.
const DMACTL_TWO_LINE: u8 = 0x2E;
/// The same with one-line P/M resolution.
const DMACTL_ONE_LINE: u8 = 0x3E;
/// Player DMA without the missile bit.
const DMACTL_PLAYERS_ONLY: u8 = 0x2A;
/// Playfield only.
const DMACTL_NO_PM: u8 = 0x22;

/// GRACTL: admit both players and missiles.
const GRACTL_BOTH: u8 = 0x03;

/// VDELAY bit 4: hold player 0 back a line.
const VDELAY_P0: u8 = 0x10;

/// The framebuffer's first pixel sits on half colour clock 69 on NTSC, so a
/// colour clock `cc` lands at pixel `2 * cc - 69`.
const FIRST_HALF_CLOCK: i32 = 69;

fn pixel_x(hpos: u8) -> i32 {
    2 * i32::from(hpos) - FIRST_HALF_CLOCK
}

/// A rectangle of pixels an object should light, inclusive at both ends.
struct Rect {
    x0: i32,
    x1: i32,
    y0: i32,
    y1: i32,
}

/// A normal-width player's eight bits at `hpos`, over rows `y0..=y1`.
fn player(hpos: u8, y0: i32, y1: i32) -> Rect {
    let x0 = pixel_x(hpos);
    Rect {
        x0,
        x1: x0 + 15,
        y0,
        y1,
    }
}

/// Player 1 is `$81`: its first and last bits only.
fn player_edges(hpos: u8, y0: i32, y1: i32) -> [Rect; 2] {
    let x0 = pixel_x(hpos);
    [
        Rect {
            x0,
            x1: x0 + 1,
            y0,
            y1,
        },
        Rect {
            x0: x0 + 14,
            x1: x0 + 15,
            y0,
            y1,
        },
    ]
}

/// A normal-width missile's two bits at `hpos`.
fn missile(hpos: u8, y0: i32, y1: i32) -> Rect {
    let x0 = pixel_x(hpos);
    Rect {
        x0,
        x1: x0 + 3,
        y0,
        y1,
    }
}

fn expected(rects: &[Rect]) -> BTreeSet<(i32, i32)> {
    let mut set = BTreeSet::new();
    for r in rects {
        for y in r.y0..=r.y1 {
            for x in r.x0..=r.x1 {
                set.insert((x, y));
            }
        }
    }
    set
}

// Where the cartridge puts things. Rows are framebuffer rows, which on NTSC
// are scan lines less eight.
const P0_HPOS: u8 = 0x80;
const P1_HPOS: u8 = 0x40;
const P2_HPOS: u8 = 0x60;
const P3_HPOS: u8 = 0xA0;
const M0_HPOS: u8 = 0xC0;

/// Player 0, player 1 and missile 0 sit on scan lines 80-95, inside the
/// mode lines.
fn objects_in_the_playfield() -> Vec<Rect> {
    let [p1a, p1b] = player_edges(P1_HPOS, 72, 87);
    vec![player(P0_HPOS, 72, 87), p1a, p1b, missile(M0_HPOS, 72, 87)]
}

/// Player 2 sits on scan lines 16-23, in the blank lines above the
/// playfield; player 3 on 232-239, below the display list's jump. ANTIC
/// fetches P/M data on every line of the display, not only on mode lines.
fn objects_outside_the_mode_lines() -> Vec<Rect> {
    vec![player(P2_HPOS, 8, 15), player(P3_HPOS, 224, 231)]
}

fn cartridge() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-data/atari/pm-dma/atari-800xl-pm-dma.bin");
    std::fs::read(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

/// Boot the probe with the given registers and return every pixel that is
/// not the background colour after the picture has settled.
fn lit_pixels(dmactl: u8, gractl: u8, vdelay: u8) -> BTreeSet<(i32, i32)> {
    let mut machine = Atari800xl::new(None, None, Some(cartridge()), Atari800xlRegion::Ntsc, false)
        .expect("cartridge-only machine");
    machine.poke(0x80, dmactl);
    machine.poke(0x81, gractl);
    machine.poke(0x82, vdelay);
    // The program sets up during the first frame; the third is whole.
    for _ in 0..3 {
        machine.run_frame();
    }

    let width = machine.framebuffer_width() as i32;
    let fb = machine.framebuffer();
    let background = fb[0];
    fb.iter()
        .enumerate()
        .filter(|&(_, &px)| px != background)
        .map(|(i, _)| (i as i32 % width, i as i32 / width))
        .collect()
}

/// Compare pixel sets and name the first few differences by position, which
/// says which object is wrong far better than two long lists would.
fn assert_lit(actual: &BTreeSet<(i32, i32)>, expected: &BTreeSet<(i32, i32)>) {
    let missing: Vec<_> = expected.difference(actual).take(8).collect();
    let extra: Vec<_> = actual.difference(expected).take(8).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "lit pixels differ from expected: {} missing (first {missing:?}), {} unexpected (first {extra:?})",
        expected.difference(actual).count(),
        actual.difference(expected).count(),
    );
}

#[test]
fn two_line_objects_reach_the_screen_where_hpos_puts_them() {
    let mut rects = objects_in_the_playfield();
    rects.extend(objects_outside_the_mode_lines());
    assert_lit(
        &lit_pixels(DMACTL_TWO_LINE, GRACTL_BOTH, 0),
        &expected(&rects),
    );
}

#[test]
fn one_line_objects_come_from_the_2k_layout() {
    // The cartridge writes each object into both layouts at positions that
    // cover the same scan lines, so the picture matches the two-line one
    // only if ANTIC reads the one-line block with its own offsets.
    let mut rects = objects_in_the_playfield();
    rects.extend(objects_outside_the_mode_lines());
    assert_lit(
        &lit_pixels(DMACTL_ONE_LINE, GRACTL_BOTH, 0),
        &expected(&rects),
    );
}

#[test]
fn vdelay_holds_a_two_line_player_back_one_line() {
    let [p1a, p1b] = player_edges(P1_HPOS, 72, 87);
    let mut rects = vec![player(P0_HPOS, 73, 88), p1a, p1b, missile(M0_HPOS, 72, 87)];
    rects.extend(objects_outside_the_mode_lines());
    assert_lit(
        &lit_pixels(DMACTL_TWO_LINE, GRACTL_BOTH, VDELAY_P0),
        &expected(&rects),
    );
}

#[test]
fn vdelay_does_nothing_at_one_line_resolution() {
    let mut rects = objects_in_the_playfield();
    rects.extend(objects_outside_the_mode_lines());
    assert_lit(
        &lit_pixels(DMACTL_ONE_LINE, GRACTL_BOTH, VDELAY_P0),
        &expected(&rects),
    );
}

#[test]
fn gractl_keeps_dma_out_of_the_graphics_registers() {
    assert_lit(&lit_pixels(DMACTL_TWO_LINE, 0, 0), &BTreeSet::new());
}

#[test]
fn player_dma_fetches_the_missiles_too() {
    // DMACTL bit 3 enables player and missile DMA together; bit 2 alone is
    // missiles only.
    let mut rects = objects_in_the_playfield();
    rects.extend(objects_outside_the_mode_lines());
    assert_lit(
        &lit_pixels(DMACTL_PLAYERS_ONLY, GRACTL_BOTH, 0),
        &expected(&rects),
    );
}

#[test]
fn without_pm_dma_the_screen_is_bare() {
    assert_lit(&lit_pixels(DMACTL_NO_PM, GRACTL_BOTH, 0), &BTreeSet::new());
}
