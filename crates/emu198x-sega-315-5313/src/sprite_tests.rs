//! Sprites in whole frames. The sprite table goes in through the data port,
//! because the chip's internal copy of each sprite's Y, size and link is
//! filled only by real VRAM writes.

use super::*;
use render::{BORDER_LEFT, LINE_WIDTH, argb};

const RED: u16 = 0x000E;
const GREEN: u16 = 0x00E0;
const BLUE: u16 = 0x0E00;
const SAT: usize = 0xD800;
const PLANE_B: usize = 0xE000;

fn display(h40: bool) -> Vdp {
    let mut vdp = Vdp::new(false);
    vdp.write_control(0x8144);
    vdp.write_control(0x8C00 | (u16::from(h40) * 0x81));
    vdp.write_control(0x8F02);
    vdp.write_control(0x8400 | (PLANE_B >> 13) as u16);
    vdp.write_control(0x8500 | (SAT >> 9) as u16);
    vdp.write_control(0x9001);
    vdp
}

/// Write words to VRAM through the port, waiting on the FIFO as a CPU would.
fn vram_write(vdp: &mut Vdp, address: usize, words: &[u16]) {
    vdp.write_control(0x4000 | (address & 0x3FFF) as u16);
    vdp.write_control((address >> 14) as u16);
    for &word in words {
        while vdp.write_data(word).is_none() {
            vdp.tick();
        }
    }
    while vdp.status() & 0x200 == 0 {
        vdp.tick();
    }
}

/// One sprite table entry: screen position, size in cells, link, and the
/// pattern word (priority, palette, flips, tile).
fn sprite(vdp: &mut Vdp, index: usize, x: i32, y: i32, cells: (u16, u16), link: u16, pattern: u16) {
    let size = ((cells.0 - 1) << 2) | (cells.1 - 1);
    vram_write(
        vdp,
        SAT + index * 8,
        &[
            (y + 128) as u16,
            size << 8 | link,
            pattern,
            (x + 128) as u16,
        ],
    );
}

fn solid_tile(vdp: &mut Vdp, tile: usize, colour: u8) {
    let byte = u16::from(colour << 4 | colour);
    vram_write(vdp, tile * 32, &[byte << 8 | byte; 16]);
}

fn frame(vdp: &mut Vdp) {
    let start = vdp.frames();
    while vdp.frames() < start + 2 {
        vdp.tick();
    }
}

fn pixel(vdp: &Vdp, x: usize, y: usize) -> u32 {
    vdp.framebuffer()[(11 + y) * LINE_WIDTH + BORDER_LEFT + x]
}

#[test]
fn a_sprite_draws_at_its_position() {
    let mut vdp = display(true);
    vdp.cram[0x21] = RED;
    solid_tile(&mut vdp, 1, 1);
    sprite(&mut vdp, 0, 40, 30, (1, 1), 0, 0x4001); // palette 2
    frame(&mut vdp);
    let red = argb(RED, 0);
    assert_eq!(pixel(&vdp, 40, 30), red);
    assert_eq!(pixel(&vdp, 47, 37), red);
    assert_eq!(pixel(&vdp, 39, 30), argb(0, 0));
    assert_eq!(pixel(&vdp, 48, 30), argb(0, 0));
    assert_eq!(pixel(&vdp, 40, 29), argb(0, 0));
    assert_eq!(pixel(&vdp, 40, 38), argb(0, 0));
}

#[test]
fn cells_run_down_each_column_and_h_flip_swaps_the_columns() {
    let mut vdp = display(true);
    vdp.cram[1] = RED;
    vdp.cram[2] = GREEN;
    vdp.cram[3] = BLUE;
    // A 2x2 sprite of tiles 1-4: column 0 is tiles 1 and 2, column 1 is 3
    // and 4.
    solid_tile(&mut vdp, 1, 1);
    solid_tile(&mut vdp, 2, 2);
    solid_tile(&mut vdp, 3, 3);
    solid_tile(&mut vdp, 4, 3);
    sprite(&mut vdp, 0, 0, 0, (2, 2), 1, 0x0001);
    sprite(&mut vdp, 1, 100, 0, (2, 2), 0, 0x0801);
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 0, 0), argb(RED, 0));
    assert_eq!(pixel(&vdp, 0, 8), argb(GREEN, 0));
    assert_eq!(pixel(&vdp, 8, 0), argb(BLUE, 0));
    assert_eq!(
        pixel(&vdp, 100, 0),
        argb(BLUE, 0),
        "flipped: column 1 first"
    );
    assert_eq!(pixel(&vdp, 108, 0), argb(RED, 0));
}

#[test]
fn earlier_sprites_in_the_list_are_in_front() {
    let mut vdp = display(true);
    vdp.cram[1] = RED;
    vdp.cram[2] = GREEN;
    solid_tile(&mut vdp, 1, 1);
    solid_tile(&mut vdp, 2, 2);
    sprite(&mut vdp, 0, 10, 10, (1, 1), 1, 0x0001);
    sprite(&mut vdp, 1, 14, 10, (1, 1), 0, 0x0002);
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 14, 10), argb(RED, 0));
    assert_eq!(pixel(&vdp, 20, 10), argb(GREEN, 0));
    assert_eq!(vdp.read_control() & 0x20, 0x20, "the overlap collides");
}

#[test]
fn a_low_priority_sprite_goes_behind_a_high_priority_plane() {
    let mut vdp = display(true);
    vdp.cram[1] = RED;
    vdp.cram[2] = GREEN;
    // The sprite's two cells are tiles 1 and 2; the plane uses tile 3.
    solid_tile(&mut vdp, 1, 1);
    solid_tile(&mut vdp, 2, 1);
    solid_tile(&mut vdp, 3, 2);
    vram_write(&mut vdp, PLANE_B, &[0x8003]);
    sprite(&mut vdp, 0, 0, 0, (2, 1), 0, 0x0001);
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 0, 0), argb(GREEN, 0), "B has priority");
    assert_eq!(pixel(&vdp, 8, 0), argb(RED, 0), "B is empty here");
}

#[test]
fn the_21st_sprite_on_a_line_is_dropped_and_overflow_set() {
    let mut vdp = display(true);
    vdp.cram[1] = RED;
    solid_tile(&mut vdp, 1, 1);
    for i in 0..21 {
        let link = if i < 20 { i as u16 + 1 } else { 0 };
        sprite(&mut vdp, i, i as i32 * 12, 50, (1, 1), link, 0x0001);
    }
    vdp.read_control();
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 19 * 12, 50), argb(RED, 0), "the 20th draws");
    assert_eq!(pixel(&vdp, 20 * 12, 50), argb(0, 0), "the 21st does not");
    assert_eq!(vdp.read_control() & 0x40, 0x40);
}

#[test]
fn h32_allows_16_sprites_a_line() {
    let mut vdp = display(false);
    vdp.cram[1] = RED;
    solid_tile(&mut vdp, 1, 1);
    for i in 0..17 {
        let link = if i < 16 { i as u16 + 1 } else { 0 };
        sprite(&mut vdp, i, i as i32 * 12, 50, (1, 1), link, 0x0001);
    }
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 15 * 12, 50), argb(RED, 0));
    assert_eq!(pixel(&vdp, 16 * 12, 50), argb(0, 0));
}

#[test]
fn h40_draws_only_40_cells_a_line() {
    let mut vdp = display(true);
    vdp.cram[1] = RED;
    // A sprite's cells are consecutive tiles: 1 to 4 here.
    for tile in 1..=4 {
        solid_tile(&mut vdp, tile, 1);
    }
    // Eleven sprites four cells wide at a 28-pixel pitch: 44 cells.
    for i in 0..11 {
        let link = if i < 10 { i as u16 + 1 } else { 0 };
        sprite(&mut vdp, i, i as i32 * 28, 60, (4, 1), link, 0x0001);
    }
    frame(&mut vdp);
    // Ten sprites take the 40 cells, covering x 0-283; the eleventh, at
    // 280, does not draw.
    let drawn = (0..320)
        .filter(|&x| pixel(&vdp, x, 60) == argb(RED, 0))
        .count();
    assert_eq!(drawn, 9 * 28 + 32);
    assert_eq!(pixel(&vdp, 290, 60), argb(0, 0));
}

#[test]
fn a_sprite_at_x_0_masks_the_sprites_after_it() {
    let mut vdp = display(true);
    vdp.cram[1] = RED;
    solid_tile(&mut vdp, 1, 1);
    sprite(&mut vdp, 0, 20, 70, (1, 1), 1, 0x0001);
    // X 0 is screen -128.
    sprite(&mut vdp, 1, -128, 70, (1, 1), 2, 0x0001);
    sprite(&mut vdp, 2, 60, 70, (1, 1), 0, 0x0001);
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 20, 70), argb(RED, 0));
    assert_eq!(pixel(&vdp, 60, 70), argb(0, 0), "masked");
}

#[test]
fn shadow_and_highlight_operators_change_what_lies_beneath() {
    let mut vdp = display(true);
    vdp.write_control(0x8C89);
    vdp.cram[1] = 0x0888;
    solid_tile(&mut vdp, 1, 1);
    solid_tile(&mut vdp, 2, 0xE);
    solid_tile(&mut vdp, 3, 0xF);
    // A high-priority plane B row, so it starts at normal brightness.
    vram_write(&mut vdp, PLANE_B, &[0x8001; 8]);
    sprite(&mut vdp, 0, 0, 0, (1, 1), 1, 0xE002); // 3:14, highlight
    sprite(&mut vdp, 1, 16, 0, (1, 1), 0, 0xE003); // 3:15, shadow
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 0, 0), argb(0x0888, 0x80));
    assert_eq!(pixel(&vdp, 8, 0), argb(0x0888, 0));
    assert_eq!(pixel(&vdp, 16, 0), argb(0x0888, 0x40));
}

#[test]
fn a_sprite_crossing_the_left_edge_is_clipped() {
    let mut vdp = display(true);
    vdp.cram[1] = RED;
    solid_tile(&mut vdp, 1, 1);
    sprite(&mut vdp, 0, -4, 0, (1, 1), 0, 0x0001);
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 3, 0), argb(RED, 0));
    assert_eq!(pixel(&vdp, 4, 0), argb(0, 0));
}

#[test]
fn a_v_flipped_tall_sprite_reverses_its_rows() {
    let mut vdp = display(true);
    vdp.cram[1] = RED;
    vdp.cram[2] = GREEN;
    solid_tile(&mut vdp, 1, 1);
    solid_tile(&mut vdp, 2, 2);
    sprite(&mut vdp, 0, 0, 0, (1, 2), 0, 0x1001);
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 0, 0), argb(GREEN, 0));
    assert_eq!(pixel(&vdp, 0, 8), argb(RED, 0));
}
