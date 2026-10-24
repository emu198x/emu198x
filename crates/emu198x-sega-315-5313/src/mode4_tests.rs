//! Mode 4 through the Master System ports, frame by frame.

use super::*;
use mode4::VINT_SLOT_MODE4;
use render::{BORDER_LEFT, LINE_WIDTH};

const NAME_TABLE: u16 = 0x3800;
const SPRITE_TABLE: u16 = 0x3F00;
/// Rows above a 192-line NTSC picture.
const TOP: usize = 27;

fn reg(vdp: &mut Vdp, reg: u8, value: u8) {
    vdp.write_control_byte(value);
    vdp.write_control_byte(0x80 | reg);
}

fn vram(vdp: &mut Vdp, address: u16, bytes: &[u8]) {
    vdp.write_control_byte(address as u8);
    vdp.write_control_byte(0x40 | (address >> 8) as u8);
    for &b in bytes {
        vdp.write_data_byte(b);
    }
}

fn cram(vdp: &mut Vdp, index: u8, colours: &[u8]) {
    vdp.write_control_byte(index);
    vdp.write_control_byte(0xC0);
    for &c in colours {
        vdp.write_data_byte(c);
    }
}

/// A VDP in Mode 4, display on, name table at `$3800`, sprites at `$3F00`
/// with their patterns at `$0000`, the sprite list empty.
fn display() -> Vdp {
    let mut vdp = Vdp::new(false);
    reg(&mut vdp, 0, 0x04);
    reg(&mut vdp, 1, 0x40);
    reg(&mut vdp, 2, 0xFF);
    reg(&mut vdp, 5, 0xFF);
    reg(&mut vdp, 6, 0xFB);
    reg(&mut vdp, 10, 0xFF);
    vram(&mut vdp, SPRITE_TABLE, &[0xD0]);
    vdp
}

/// A tile of one colour.
fn solid_tile(vdp: &mut Vdp, tile: u16, colour: u8) {
    let row: [u8; 4] = std::array::from_fn(|plane| if colour >> plane & 1 != 0 { 0xFF } else { 0 });
    let bytes: Vec<u8> = (0..8).flat_map(|_| row).collect();
    vram(vdp, tile * 32, &bytes);
}

/// The name table entry at a cell.
fn name(vdp: &mut Vdp, column: u16, row: u16, entry: u16) {
    vram(
        vdp,
        NAME_TABLE + (row * 32 + column) * 2,
        &[entry as u8, (entry >> 8) as u8],
    );
}

/// Place sprite `index` at (`x`, `y`), the SAT's raw values.
fn sprite(vdp: &mut Vdp, index: u16, x: u8, y: u8, tile: u8) {
    vram(vdp, SPRITE_TABLE + index, &[y]);
    vram(vdp, SPRITE_TABLE + 0x80 + index * 2, &[x, tile]);
}

fn frame(vdp: &mut Vdp) {
    let start = vdp.frames();
    while vdp.frames() < start + 2 {
        vdp.tick();
    }
}

fn pixel(vdp: &Vdp, x: usize, y: usize) -> u32 {
    vdp.framebuffer()[(TOP + y) * LINE_WIDTH + BORDER_LEFT + x]
}

const RED: u8 = 0x03;
const GREEN: u8 = 0x0C;
const BLUE: u8 = 0x30;
const WHITE: u8 = 0x3F;

#[test]
fn tiles_draw_from_the_name_table_with_their_palette_and_flips() {
    let mut vdp = display();
    cram(&mut vdp, 0, &[0x00, RED]);
    cram(&mut vdp, 17, &[GREEN]);
    solid_tile(&mut vdp, 1, 1);
    // A tile whose top row is colour 1 and the rest 0, to see the flip.
    let mut top_row = vec![0u8; 32];
    top_row[0] = 0xFF;
    vram(&mut vdp, 2 * 32, &top_row);
    name(&mut vdp, 0, 0, 1);
    name(&mut vdp, 1, 0, 1 | 0x0800); // palette 1
    name(&mut vdp, 2, 0, 2 | 0x0400); // vertical flip
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 0, 0), argb_mode4(RED));
    assert_eq!(pixel(&vdp, 7, 7), argb_mode4(RED));
    assert_eq!(pixel(&vdp, 8, 0), argb_mode4(GREEN), "palette 1");
    assert_eq!(
        pixel(&vdp, 16, 0),
        argb_mode4(0),
        "flipped: the top row is blank"
    );
    assert_eq!(pixel(&vdp, 16, 7), argb_mode4(RED), "and the bottom set");
    assert_eq!(pixel(&vdp, 24, 0), argb_mode4(0), "tile 0, colour 0");
    assert_eq!(vdp.frame_width(), MODE4_FRAME_WIDTH);
    assert_eq!(vdp.frame_height(), 243);
}

#[test]
fn scrolling_moves_the_layer_and_the_locks_hold_it() {
    let mut vdp = display();
    cram(&mut vdp, 0, &[0x00, RED]);
    solid_tile(&mut vdp, 1, 1);
    name(&mut vdp, 0, 0, 1);
    name(&mut vdp, 0, 4, 1);
    reg(&mut vdp, 8, 3);
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 2, 32), argb_mode4(0), "pushed right by three");
    assert_eq!(pixel(&vdp, 3, 32), argb_mode4(RED));
    assert_eq!(pixel(&vdp, 10, 32), argb_mode4(RED));
    // With the top two rows locked, the first row does not scroll.
    reg(&mut vdp, 0, 0x44);
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 0, 0), argb_mode4(RED));
    assert_eq!(pixel(&vdp, 3, 32), argb_mode4(RED), "row 4 still scrolls");
    // Vertical scroll: row 4's tile rises to row 0.
    reg(&mut vdp, 0, 0x04);
    reg(&mut vdp, 8, 0);
    name(&mut vdp, 0, 0, 0);
    reg(&mut vdp, 9, 32);
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 0, 0), argb_mode4(RED));
}

#[test]
fn the_left_column_can_be_blanked_to_the_border_colour() {
    let mut vdp = display();
    cram(&mut vdp, 0, &[0x00, RED]);
    cram(&mut vdp, 16 + 5, &[BLUE]);
    reg(&mut vdp, 7, 5);
    solid_tile(&mut vdp, 1, 1);
    name(&mut vdp, 0, 0, 1);
    reg(&mut vdp, 0, 0x24);
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 0, 0), argb_mode4(BLUE));
    assert_eq!(pixel(&vdp, 7, 0), argb_mode4(BLUE));
    assert_eq!(pixel(&vdp, 8, 0), argb_mode4(0));
    // The border itself is the same colour.
    assert_eq!(vdp.framebuffer()[TOP * LINE_WIDTH], argb_mode4(BLUE));
    assert_eq!(vdp.framebuffer()[0], argb_mode4(BLUE), "the top border");
}

#[test]
fn a_sprite_shows_from_the_line_after_its_y() {
    let mut vdp = display();
    cram(&mut vdp, 16 + 2, &[WHITE]);
    solid_tile(&mut vdp, 4, 2);
    sprite(&mut vdp, 0, 40, 9, 4);
    vram(&mut vdp, SPRITE_TABLE + 1, &[0xD0]);
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 40, 9), argb_mode4(0), "Y is the line above");
    assert_eq!(pixel(&vdp, 40, 10), argb_mode4(WHITE));
    assert_eq!(pixel(&vdp, 47, 17), argb_mode4(WHITE));
    assert_eq!(pixel(&vdp, 48, 10), argb_mode4(0));
    assert_eq!(pixel(&vdp, 40, 18), argb_mode4(0));
    // The early-clock bit shifts sprites eight pixels left.
    reg(&mut vdp, 0, 0x0C);
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 32, 10), argb_mode4(WHITE));
    assert_eq!(pixel(&vdp, 40, 10), argb_mode4(0));
}

#[test]
fn tall_sprites_take_a_tile_pair_and_never_zoom() {
    let mut vdp = display();
    cram(&mut vdp, 16 + 1, &[RED, GREEN]);
    solid_tile(&mut vdp, 6, 1);
    solid_tile(&mut vdp, 7, 2);
    reg(&mut vdp, 1, 0x43); // 8x16 sprites, zoom asked for
    sprite(&mut vdp, 0, 0, 19, 7); // the odd tile number is rounded down
    vram(&mut vdp, SPRITE_TABLE + 1, &[0xD0]);
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 0, 20), argb_mode4(RED));
    assert_eq!(pixel(&vdp, 0, 28), argb_mode4(GREEN));
    assert_eq!(pixel(&vdp, 8, 20), argb_mode4(0), "not doubled");
    assert_eq!(pixel(&vdp, 0, 36), argb_mode4(0));
}

#[test]
fn a_background_tile_with_priority_covers_sprites_but_not_with_colour_0() {
    let mut vdp = display();
    cram(&mut vdp, 0, &[0x00, RED]);
    cram(&mut vdp, 16 + 3, &[WHITE]);
    // A tile with its left half colour 1 and right half colour 0.
    let row = [0xF0, 0, 0, 0];
    let bytes: Vec<u8> = (0..8).flat_map(|_| row).collect();
    vram(&mut vdp, 32, &bytes);
    name(&mut vdp, 0, 2, 1 | 0x1000);
    solid_tile(&mut vdp, 5, 3);
    sprite(&mut vdp, 0, 0, 15, 5);
    vram(&mut vdp, SPRITE_TABLE + 1, &[0xD0]);
    frame(&mut vdp);
    assert_eq!(
        pixel(&vdp, 0, 16),
        argb_mode4(RED),
        "priority colour in front"
    );
    assert_eq!(pixel(&vdp, 4, 16), argb_mode4(WHITE), "colour 0 is not");
}

#[test]
fn the_ninth_sprite_on_a_line_is_dropped_and_overflow_set() {
    let mut vdp = display();
    cram(&mut vdp, 16 + 1, &[RED]);
    solid_tile(&mut vdp, 1, 1);
    for i in 0..9 {
        sprite(&mut vdp, i, (i * 16) as u8, 49, 1);
    }
    vram(&mut vdp, SPRITE_TABLE + 9, &[0xD0]);
    vdp.read_status_byte();
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 7 * 16, 50), argb_mode4(RED), "the eighth draws");
    assert_eq!(pixel(&vdp, 8 * 16, 50), argb_mode4(0), "the ninth does not");
    assert_eq!(vdp.read_status_byte() & 0x40, 0x40);
    assert_eq!(vdp.read_status_byte() & 0x40, 0, "reading clears it");
}

#[test]
fn overlapping_sprites_collide_and_the_first_stays_in_front() {
    let mut vdp = display();
    cram(&mut vdp, 16 + 1, &[RED, GREEN]);
    solid_tile(&mut vdp, 1, 1);
    solid_tile(&mut vdp, 2, 2);
    sprite(&mut vdp, 0, 10, 59, 1);
    sprite(&mut vdp, 1, 14, 59, 2);
    vram(&mut vdp, SPRITE_TABLE + 2, &[0xD0]);
    vdp.read_status_byte();
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 14, 60), argb_mode4(RED));
    assert_eq!(pixel(&vdp, 20, 60), argb_mode4(GREEN));
    assert_eq!(vdp.read_status_byte() & 0x20, 0x20);
}

#[test]
fn y_d0_ends_the_sprite_list() {
    let mut vdp = display();
    cram(&mut vdp, 16 + 1, &[RED]);
    solid_tile(&mut vdp, 1, 1);
    sprite(&mut vdp, 0, 0, 0xD0, 1);
    sprite(&mut vdp, 1, 50, 69, 1);
    frame(&mut vdp);
    assert_eq!(pixel(&vdp, 50, 70), argb_mode4(0));
}

#[test]
fn the_frame_interrupt_comes_on_the_line_after_the_picture() {
    let mut vdp = display();
    reg(&mut vdp, 1, 0x60);
    frame(&mut vdp);
    while vdp.read_v_counter() != 0 {
        vdp.tick();
    }
    vdp.read_status_byte();
    while !vdp.z80_interrupt() {
        vdp.tick();
    }
    assert_eq!(vdp.vcounter(), 193);
    assert_eq!(vdp.hslot(), VINT_SLOT_MODE4);
    let status = vdp.read_status_byte();
    assert_eq!(status & 0x80, 0x80);
    assert!(!vdp.z80_interrupt(), "reading the status clears it");
}

#[test]
fn the_line_interrupt_counts_register_10_lines() {
    let mut vdp = display();
    reg(&mut vdp, 10, 9);
    reg(&mut vdp, 0, 0x14);
    frame(&mut vdp);
    while vdp.read_v_counter() != 0 {
        vdp.tick();
    }
    vdp.read_status_byte();
    while !vdp.z80_interrupt() {
        vdp.tick();
    }
    // Loaded with 9 in the blank, the counter underflows on line 9.
    assert_eq!(vdp.read_v_counter(), 9);
    vdp.read_status_byte();
    while !vdp.z80_interrupt() {
        vdp.tick();
    }
    assert_eq!(vdp.read_v_counter(), 19);
}

#[test]
fn the_v_counter_reads_as_a_master_systems() {
    let mut vdp = display();
    let mut seen = Vec::new();
    let mut last = vdp.read_v_counter();
    for _ in 0..262 * 3420 {
        vdp.tick();
        let v = vdp.read_v_counter();
        if v != last {
            seen.push(v);
            last = v;
        }
    }
    let jump = seen.windows(2).find(|w| w[1] != w[0].wrapping_add(1));
    assert_eq!(jump, Some(&[0xDA, 0xD5][..]));
}

#[test]
fn the_data_port_reads_through_a_buffer() {
    let mut vdp = display();
    vram(&mut vdp, 0x1000, &[0x11, 0x22, 0x33]);
    vdp.write_control_byte(0x00);
    vdp.write_control_byte(0x10);
    assert_eq!(vdp.read_data_byte(), 0x11, "code 0 fills the buffer");
    assert_eq!(vdp.read_data_byte(), 0x22);
    assert_eq!(vdp.read_data_byte(), 0x33);
}

#[test]
fn a_taller_picture_needs_m2_and_one_of_m1_or_m3() {
    let mut vdp = display();
    assert_eq!(vdp.mode4_height(), 192);
    reg(&mut vdp, 1, 0x50);
    assert_eq!(vdp.mode4_height(), 192, "M1 without M2");
    reg(&mut vdp, 0, 0x06);
    assert_eq!(vdp.mode4_height(), 224);
    reg(&mut vdp, 1, 0x48);
    assert_eq!(vdp.mode4_height(), 240);
    reg(&mut vdp, 1, 0x58);
    assert_eq!(vdp.mode4_height(), 192, "both is 192");
    reg(&mut vdp, 1, 0x50);
    frame(&mut vdp);
    assert_eq!(vdp.frame_height(), 243);
}

#[test]
fn with_neither_mode_the_picture_is_black() {
    let mut vdp = Vdp::new(false);
    reg(&mut vdp, 1, 0x40);
    frame(&mut vdp);
    assert!(
        vdp.framebuffer()[..LINE_WIDTH * 200]
            .chunks(LINE_WIDTH)
            .all(|row| row[..MODE4_FRAME_WIDTH].iter().all(|&p| p == 0xFF00_0000))
    );
}

#[test]
fn registers_past_10_cannot_be_written_in_mode_4() {
    let mut vdp = display();
    reg(&mut vdp, 0x0F, 2);
    assert_eq!(vdp.register(0x0F), 0);
}
