//! Whole frames: memory set up directly, a frame run, pixels checked.

use super::*;
use render::{BORDER_LEFT, LINE_WIDTH, argb};

const RED: u16 = 0x000E;
const GREEN: u16 = 0x00E0;
const BLUE: u16 = 0x0E00;
const GREY: u16 = 0x0666;

const PLANE_A: usize = 0xC000;
const PLANE_B: usize = 0xE000;
const WINDOW: usize = 0xB000;
const HSCROLL: usize = 0xFC00;

/// An NTSC VDP in Mode 5, display on, with the plane tables at the usual
/// addresses and 64x32 planes.
fn display(h40: bool) -> Vdp {
    let mut vdp = Vdp::new(false);
    vdp.write_control(0x8144);
    vdp.write_control(0x8C00 | (u16::from(h40) * 0x81));
    vdp.write_control(0x8200 | (PLANE_A >> 10) as u16);
    vdp.write_control(0x8300 | (WINDOW >> 10) as u16);
    vdp.write_control(0x8400 | (PLANE_B >> 13) as u16);
    vdp.write_control(0x8D00 | (HSCROLL >> 10) as u16);
    vdp.write_control(0x9001);
    vdp
}

/// Tile `tile`: every pixel colour `colour`, or a pattern from `rows`.
fn solid_tile(vdp: &mut Vdp, tile: usize, colour: u8) {
    let byte = colour << 4 | colour;
    vdp.vram[tile * 32..tile * 32 + 32].fill(byte);
}

/// A tile whose left pixel of every row is colour 1 and the rest colour 2,
/// and whose top row is colour 3: it shows which way it is flipped.
fn marker_tile(vdp: &mut Vdp, tile: usize) {
    for row in 0..8 {
        let bytes: [u8; 4] = if row == 0 {
            [0x33; 4]
        } else {
            [0x12, 0x22, 0x22, 0x22]
        };
        vdp.vram[tile * 32 + row * 4..tile * 32 + row * 4 + 4].copy_from_slice(&bytes);
    }
}

fn map(vdp: &mut Vdp, base: usize, width: usize, x: usize, y: usize, entry: u16) {
    let address = base + (y * width + x) * 2;
    vdp.vram[address] = (entry >> 8) as u8;
    vdp.vram[address + 1] = entry as u8;
}

/// Run until a whole frame has been output.
fn frame(vdp: &mut Vdp) {
    let start = vdp.frames();
    while vdp.frames() < start + 2 {
        vdp.tick();
    }
}

fn top(vdp: &Vdp) -> usize {
    if vdp.pal_machine { 38 } else { 11 }
}

/// The pixel at picture position (x, y).
fn pixel(vdp: &Vdp, x: usize, y: usize) -> u32 {
    vdp.framebuffer()[(top(vdp) + y) * LINE_WIDTH + BORDER_LEFT + x]
}

#[test]
fn an_empty_picture_is_the_backdrop_border_and_all() {
    let mut vdp = display(true);
    vdp.cram[0x15] = GREY;
    vdp.write_control(0x8715);
    frame(&mut vdp);
    let grey = argb(GREY, 0);
    assert_eq!(pixel(&vdp, 0, 0), grey);
    assert_eq!(pixel(&vdp, 319, 223), grey);
    assert_eq!(vdp.framebuffer()[0], grey, "top-left border");
    assert_eq!(vdp.frame_width(), 347);
    assert_eq!(vdp.frame_height(), 243);
}

#[test]
fn a_plane_b_tile_appears_at_its_map_position() {
    let mut vdp = display(true);
    vdp.cram[1] = RED;
    solid_tile(&mut vdp, 1, 1);
    map(&mut vdp, PLANE_B, 64, 3, 2, 1);
    frame(&mut vdp);
    let red = argb(RED, 0);
    let black = argb(0, 0);
    assert_eq!(pixel(&vdp, 24, 16), red);
    assert_eq!(pixel(&vdp, 31, 23), red);
    assert_eq!(pixel(&vdp, 23, 16), black);
    assert_eq!(pixel(&vdp, 32, 16), black);
    assert_eq!(pixel(&vdp, 24, 24), black);
}

#[test]
fn flips_mirror_the_pattern() {
    let mut vdp = display(true);
    vdp.cram[1] = RED;
    vdp.cram[2] = GREEN;
    vdp.cram[3] = BLUE;
    marker_tile(&mut vdp, 1);
    map(&mut vdp, PLANE_B, 64, 0, 0, 0x0001);
    map(&mut vdp, PLANE_B, 64, 1, 0, 0x0801); // H flip
    map(&mut vdp, PLANE_B, 64, 2, 0, 0x1001); // V flip
    frame(&mut vdp);
    let (red, green, blue) = (argb(RED, 0), argb(GREEN, 0), argb(BLUE, 0));
    assert_eq!(
        (pixel(&vdp, 0, 0), pixel(&vdp, 0, 1), pixel(&vdp, 7, 1)),
        (blue, red, green)
    );
    assert_eq!(
        (pixel(&vdp, 8, 1), pixel(&vdp, 15, 1)),
        (green, red),
        "H flip"
    );
    assert_eq!(
        (pixel(&vdp, 16, 7), pixel(&vdp, 16, 0)),
        (blue, red),
        "V flip"
    );
}

#[test]
fn plane_a_covers_b_unless_b_has_priority() {
    let mut vdp = display(true);
    vdp.cram[1] = RED;
    vdp.cram[0x12] = GREEN;
    solid_tile(&mut vdp, 1, 1);
    solid_tile(&mut vdp, 2, 2);
    map(&mut vdp, PLANE_B, 64, 0, 0, 0x0001);
    map(&mut vdp, PLANE_A, 64, 0, 0, 0x2002); // palette 1
    map(&mut vdp, PLANE_B, 64, 1, 0, 0x8001); // B with priority
    map(&mut vdp, PLANE_A, 64, 1, 0, 0x2002);
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 0, 0), argb(GREEN, 0));
    assert_eq!(pixel(&vdp, 8, 0), argb(RED, 0));
}

#[test]
fn fine_and_coarse_horizontal_scroll_move_the_plane_right() {
    let mut vdp = display(true);
    vdp.cram[1] = RED;
    solid_tile(&mut vdp, 1, 1);
    map(&mut vdp, PLANE_B, 64, 0, 0, 1);
    // Plane B scrolled 19 pixels right: two cells and three pixels.
    vdp.vram[HSCROLL + 2] = 0;
    vdp.vram[HSCROLL + 3] = 19;
    frame(&mut vdp);
    let red = argb(RED, 0);
    assert_eq!(pixel(&vdp, 18, 0), argb(0, 0));
    assert_eq!(pixel(&vdp, 19, 0), red);
    assert_eq!(pixel(&vdp, 26, 0), red);
    assert_eq!(pixel(&vdp, 27, 0), argb(0, 0));
}

#[test]
fn per_line_horizontal_scroll_reads_a_value_per_line() {
    let mut vdp = display(true);
    vdp.write_control(0x8B03);
    vdp.cram[1] = RED;
    solid_tile(&mut vdp, 1, 1);
    map(&mut vdp, PLANE_B, 64, 0, 0, 1);
    for line in 0..8 {
        vdp.vram[HSCROLL + line * 4 + 3] = line as u8;
    }
    frame(&mut vdp);
    for line in 0..8 {
        assert_eq!(
            pixel(&vdp, line, line),
            argb(RED, 0),
            "line {line} starts at x {line}"
        );
        if line > 0 {
            assert_eq!(pixel(&vdp, line - 1, line), argb(0, 0));
        }
    }
}

#[test]
fn vertical_scroll_moves_the_plane_up() {
    let mut vdp = display(true);
    vdp.cram[1] = RED;
    solid_tile(&mut vdp, 1, 1);
    map(&mut vdp, PLANE_B, 64, 0, 2, 1); // rows 16-23
    vdp.vsram[1] = 5;
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 0, 10), argb(0, 0));
    assert_eq!(pixel(&vdp, 0, 11), argb(RED, 0));
    assert_eq!(pixel(&vdp, 0, 18), argb(RED, 0));
    assert_eq!(pixel(&vdp, 0, 19), argb(0, 0));
}

#[test]
fn two_cell_vertical_scroll_scrolls_each_column_on_its_own() {
    let mut vdp = display(true);
    vdp.write_control(0x8B04);
    vdp.cram[1] = RED;
    solid_tile(&mut vdp, 1, 1);
    map(&mut vdp, PLANE_B, 64, 2, 1, 1); // columns 16-23, rows 8-15
    map(&mut vdp, PLANE_B, 64, 4, 1, 1); // columns 32-39
    vdp.vsram[3] = 8; // B, second two-cell column
    frame(&mut vdp);
    assert_eq!(
        pixel(&vdp, 16, 0),
        argb(RED, 0),
        "this column scrolled up a cell"
    );
    assert_eq!(pixel(&vdp, 32, 8), argb(RED, 0), "this one did not");
    assert_eq!(pixel(&vdp, 32, 0), argb(0, 0));
}

#[test]
fn the_window_replaces_plane_a_where_it_covers() {
    let mut vdp = display(true);
    vdp.cram[1] = RED;
    vdp.cram[2] = GREEN;
    solid_tile(&mut vdp, 1, 1);
    solid_tile(&mut vdp, 2, 2);
    for x in 0..40 {
        map(&mut vdp, PLANE_A, 64, x, 0, 1);
        map(&mut vdp, WINDOW, 64, x, 0, 2);
    }
    // The window: the left four columns (eight cells).
    vdp.write_control(0x9104);
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 0, 0), argb(GREEN, 0));
    assert_eq!(pixel(&vdp, 63, 0), argb(GREEN, 0));
    assert_eq!(pixel(&vdp, 64, 0), argb(RED, 0));
}

#[test]
fn the_column_0_mask_blanks_the_first_eight_pixels() {
    let mut vdp = display(true);
    vdp.cram[1] = RED;
    solid_tile(&mut vdp, 1, 1);
    map(&mut vdp, PLANE_B, 64, 0, 0, 1);
    map(&mut vdp, PLANE_B, 64, 1, 0, 1);
    vdp.write_control(0x8024);
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 7, 0), argb(0, 0));
    assert_eq!(pixel(&vdp, 8, 0), argb(RED, 0));
}

#[test]
fn shadow_mode_darkens_low_priority_pixels() {
    let mut vdp = display(true);
    vdp.write_control(0x8C89);
    vdp.cram[1] = 0x0EEE;
    solid_tile(&mut vdp, 1, 1);
    map(&mut vdp, PLANE_B, 64, 0, 0, 0x0001);
    map(&mut vdp, PLANE_B, 64, 1, 0, 0x8001);
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 0, 0), argb(0x0EEE, 0x40));
    assert_eq!(pixel(&vdp, 8, 0), argb(0x0EEE, 0));
}

#[test]
fn h32_lines_are_283_pixels_wide() {
    let mut vdp = display(false);
    vdp.cram[1] = RED;
    solid_tile(&mut vdp, 1, 1);
    map(&mut vdp, PLANE_B, 64, 31, 0, 1);
    frame(&mut vdp);
    assert_eq!(vdp.frame_width(), 283);
    assert_eq!(pixel(&vdp, 248, 0), argb(RED, 0));
    assert_eq!(pixel(&vdp, 255, 0), argb(RED, 0));
    assert_eq!(pixel(&vdp, 256, 0), argb(0, 0), "right border");
}

#[test]
fn a_mid_line_cram_write_changes_the_colour_from_where_it_lands() {
    let mut vdp = display(true);
    vdp.cram[0] = RED;
    // Wait for the middle of a picture line, then write CRAM 0.
    frame(&mut vdp);
    while !(vdp.vcounter() == 100 && vdp.hslot() == 60) {
        vdp.tick();
    }
    vdp.write_control(0xC000);
    vdp.write_control(0x0000);
    vdp.write_data(GREEN).expect("room");
    while vdp.vcounter() != 102 {
        vdp.tick();
    }
    let row = &vdp.framebuffer()[(11 + 100) * LINE_WIDTH..(11 + 101) * LINE_WIDTH];
    let first_green = row
        .iter()
        .position(|&p| p == argb(GREEN, 0))
        .expect("green lands");
    assert!(row[..first_green].iter().all(|&p| p == argb(RED, 0)));
    // Left of where the write's slot outputs, plus its latency.
    assert!(
        (100..140).contains(&first_green),
        "lands near slot 60: x {first_green}"
    );
}

#[test]
fn the_display_off_shows_the_backdrop() {
    let mut vdp = display(true);
    vdp.cram[1] = RED;
    vdp.cram[5] = BLUE;
    vdp.write_control(0x8705);
    solid_tile(&mut vdp, 1, 1);
    map(&mut vdp, PLANE_B, 64, 0, 0, 1);
    vdp.write_control(0x8104);
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 0, 0), argb(BLUE, 0));
}

#[test]
fn a_right_hand_window_and_a_bottom_window_cover_where_they_say() {
    let mut vdp = display(true);
    vdp.cram[1] = RED;
    vdp.cram[2] = GREEN;
    solid_tile(&mut vdp, 1, 1);
    solid_tile(&mut vdp, 2, 2);
    for y in 0..28 {
        for x in 0..40 {
            map(&mut vdp, PLANE_A, 64, x, y, 1);
            map(&mut vdp, WINDOW, 64, x, y, 2);
        }
    }
    // Right of column 16 (cell 32).
    vdp.write_control(0x9190);
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 255, 0), argb(RED, 0));
    assert_eq!(pixel(&vdp, 256 + 8, 0), argb(GREEN, 0));
    // Below cell row 20 (line 160).
    vdp.write_control(0x9100);
    vdp.write_control(0x9294);
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 0, 159), argb(RED, 0));
    assert_eq!(pixel(&vdp, 0, 160), argb(GREEN, 0));
}

#[test]
fn an_h32_window_uses_the_32_cell_table() {
    let mut vdp = display(false);
    vdp.cram[2] = GREEN;
    solid_tile(&mut vdp, 2, 2);
    // Row 1 of a 32-cell-wide window table.
    map(&mut vdp, WINDOW, 32, 0, 1, 2);
    vdp.write_control(0x9202); // the top two cell rows
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 0, 8), argb(GREEN, 0));
    assert_eq!(pixel(&vdp, 0, 0), argb(0, 0));
}

#[test]
fn interlace_mode_2_draws_16_line_tiles_on_alternate_fields() {
    let mut vdp = display(true);
    vdp.write_control(0x8C87); // H40, interlace mode 2
    vdp.cram[1] = RED;
    // A 16-line tile 1 (64 bytes) whose lower half is colour 1.
    vdp.vram[64 + 32..128].fill(0x11);
    map(&mut vdp, PLANE_B, 64, 0, 0, 1);
    frame(&mut vdp);
    // Each field shows every other line of the 16-line tile: lines 4-7.
    assert_eq!(pixel(&vdp, 0, 3), argb(0, 0));
    assert_eq!(pixel(&vdp, 0, 4), argb(RED, 0));
}

#[test]
fn a_pal_v30_frame_has_240_picture_lines() {
    let mut vdp = Vdp::new(true);
    vdp.write_control(0x814C);
    vdp.write_control(0x8C81);
    vdp.cram[0x01] = RED;
    vdp.write_control(0x8701);
    frame(&mut vdp);
    assert_eq!(vdp.frame_height(), 294);
    assert_eq!(
        vdp.framebuffer()[(30 + 239) * LINE_WIDTH + 20],
        argb(RED, 0)
    );
}
