//! The Mode 4 sprite size matrix, from `vdp-sms-reference.md`:
//!
//! | SZ (R1.1) | MAG (R1.0) | On-screen size | Tile fetches |
//! |---|---|---|---|
//! | 0 | 0 | 8 x 8   | 1 tile |
//! | 0 | 1 | 16 x 16 | 1 tile, doubled |
//! | 1 | 0 | 8 x 16  | 2 tiles (even+odd pair, pattern index LSB forced to 0) |
//! | 1 | 1 | 16 x 32 | 2 tiles, doubled |
//!
//! Magnification doubles the ground a sprite covers without changing the
//! pattern it comes from, so the whole matrix is two independent bits and
//! the test walks all four corners of it.

use sega_vdp::{SegaVdp, VdpRegion, VdpVariant};

const REGION: VdpRegion = VdpRegion::Ntsc;

fn write_register(vdp: &mut SegaVdp, reg: u8, value: u8) {
    vdp.write_control(value);
    vdp.write_control(0x80 | (reg & 0x0F));
}

fn poke_vram(vdp: &mut SegaVdp, addr: u16, bytes: &[u8]) {
    vdp.write_control(addr as u8);
    vdp.write_control(((addr >> 8) as u8 & 0x3F) | 0x40);
    for &b in bytes {
        vdp.write_data(b);
    }
}

fn poke_cram(vdp: &mut SegaVdp, index: u8, value: u8) {
    vdp.write_control(index);
    vdp.write_control(0xC0);
    vdp.write_data(value);
}

fn solid_tile(colour: u8) -> [u8; 32] {
    let plane = |bit: u8| if colour & (1 << bit) != 0 { 0xFF } else { 0x00 };
    let row = [plane(0), plane(1), plane(2), plane(3)];
    let mut tile = [0u8; 32];
    for r in 0..8 {
        tile[r * 4..r * 4 + 4].copy_from_slice(&row);
    }
    tile
}

fn render_frame(vdp: &mut SegaVdp) {
    while !vdp.tick_scanline() {}
    for _ in 0..192 {
        vdp.tick_scanline();
    }
}

fn pixel(vdp: &SegaVdp, line: u32, x: u32) -> u32 {
    let index =
        (REGION.border_top() + line) * REGION.framebuffer_width() + REGION.border_left() + x;
    vdp.framebuffer()[index as usize]
}

fn sms_argb(entry: u8) -> u32 {
    let level = |c: u32| c * 85;
    0xFF00_0000
        | (level(u32::from(entry) & 3) << 16)
        | (level((u32::from(entry) >> 2) & 3) << 8)
        | level((u32::from(entry) >> 4) & 3)
}

const BACKDROP: u8 = 0x30;
const INK: u8 = 0x03;

/// One sprite at the top-left corner, drawn from pattern 2 — an even index,
/// so an 8x16 sprite pairs it with pattern 3.
fn one_sprite(reg1: u8) -> SegaVdp {
    let mut vdp = SegaVdp::new(REGION, VdpVariant::Sms2);
    write_register(&mut vdp, 0, 0x04); // Mode 4
    write_register(&mut vdp, 1, 0x40 | reg1); // display on, plus SZ/MAG
    write_register(&mut vdp, 5, 0xFF); // attribute table $3F00
    write_register(&mut vdp, 6, 0x00); // sprite patterns $0000
    write_register(&mut vdp, 7, 0x00); // backdrop = CRAM entry 16
    poke_cram(&mut vdp, 16, BACKDROP);
    poke_cram(&mut vdp, 16 + 5, INK);
    poke_vram(&mut vdp, 0x0040, &solid_tile(5)); // pattern 2
    poke_vram(&mut vdp, 0x0060, &solid_tile(5)); // pattern 3, its 8x16 partner
    poke_vram(&mut vdp, 0x3F00, &[15, 0xD0]); // sprite 0 starts on line 16
    poke_vram(&mut vdp, 0x3F80, &[0, 2]); // x = 0, pattern 2
    vdp
}

/// Measure the sprite's extent down and across from its top-left corner.
fn extent(vdp: &SegaVdp) -> (u32, u32) {
    let ink = sms_argb(INK);
    let height = (16..192).take_while(|&y| pixel(vdp, y, 0) == ink).count() as u32;
    let width = (0..256).take_while(|&x| pixel(vdp, 16, x) == ink).count() as u32;
    (width, height)
}

#[test]
fn the_four_corners_of_the_sprite_size_matrix() {
    for (sz, mag, expected) in [
        (0u8, 0u8, (8u32, 8u32)),
        (0, 1, (16, 16)),
        (1, 0, (8, 16)),
        (1, 1, (16, 32)),
    ] {
        let mut vdp = one_sprite((sz << 1) | mag);
        render_frame(&mut vdp);
        assert_eq!(
            extent(&vdp),
            expected,
            "SZ={sz} MAG={mag} should draw a {}x{} sprite",
            expected.0,
            expected.1
        );
    }
}

/// Magnification doubles pixels; it does not fetch a second pattern. Give
/// the 8x16 partner pattern a different colour and a magnified 8x8 must
/// never show it, however tall it grows.
#[test]
fn magnification_doubles_the_pattern_rather_than_fetching_another() {
    let mut vdp = one_sprite(0x01); // MAG, no SZ
    poke_vram(&mut vdp, 0x0060, &solid_tile(6)); // pattern 3 in another colour
    poke_cram(&mut vdp, 16 + 6, 0x0C);
    render_frame(&mut vdp);

    for y in 16..32u32 {
        assert_eq!(
            pixel(&vdp, y, 0),
            sms_argb(INK),
            "line {y} of a magnified 8x8 must come from its own pattern"
        );
    }
    assert_eq!(
        pixel(&vdp, 32, 0),
        sms_argb(BACKDROP),
        "and it must stop after sixteen lines"
    );
}

/// Each pattern row is drawn twice over, not stretched by interpolation, so
/// a one-pixel feature becomes a two-pixel block in both directions.
#[test]
fn magnification_repeats_each_pixel_exactly_once() {
    let mut vdp = one_sprite(0x01);
    // Pattern 2, row 0: a single leftmost pixel of colour 5. Every later row
    // is blank, so the shape is one dot.
    let mut tile = [0u8; 32];
    tile[0] = 0x80; // plane 0, bit 7
    tile[2] = 0x80; // plane 2, bit 7  -> colour 5
    poke_vram(&mut vdp, 0x0040, &tile);
    render_frame(&mut vdp);

    let ink = sms_argb(INK);
    let bg = sms_argb(BACKDROP);
    assert_eq!(
        [
            pixel(&vdp, 16, 0),
            pixel(&vdp, 16, 1),
            pixel(&vdp, 17, 0),
            pixel(&vdp, 17, 1)
        ],
        [ink; 4],
        "one pattern pixel should fill a 2x2 block"
    );
    assert_eq!(pixel(&vdp, 16, 2), bg, "and be two pixels wide, not more");
    assert_eq!(pixel(&vdp, 18, 0), bg, "and two lines tall, not more");
}

/// A magnified sprite is on twice as many lines as its pattern has rows, so
/// it occupies one of the eight per-line slots across all of them. Eight
/// magnified sprites fill the line; a ninth overflows exactly as an
/// unmagnified one would.
#[test]
fn magnified_sprites_hold_their_slot_for_every_line_they_cover() {
    let mut vdp = SegaVdp::new(REGION, VdpVariant::Sms2);
    write_register(&mut vdp, 0, 0x04);
    write_register(&mut vdp, 1, 0x41); // display on + MAG
    write_register(&mut vdp, 5, 0xFF);
    write_register(&mut vdp, 6, 0x00);
    write_register(&mut vdp, 7, 0x00);
    poke_cram(&mut vdp, 16, BACKDROP);
    poke_cram(&mut vdp, 16 + 5, INK);
    poke_vram(&mut vdp, 0x0020, &solid_tile(5));
    for i in 0..9u8 {
        poke_vram(&mut vdp, 0x3F00 + u16::from(i), &[15]);
        poke_vram(&mut vdp, 0x3F80 + u16::from(i) * 2, &[i * 16, 1]);
    }
    poke_vram(&mut vdp, 0x3F09, &[0xD0]);

    render_frame(&mut vdp);
    assert_eq!(
        vdp.read_status() & 0x40,
        0x40,
        "nine magnified sprites on a line still overflow"
    );
    // Line 30 is inside the doubled height but past the pattern's own eight
    // rows, so it only holds sprites if magnification extended their reach.
    assert_eq!(
        pixel(&vdp, 30, 0),
        sms_argb(INK),
        "the sprites are still being drawn on line 30"
    );
    assert_eq!(
        pixel(&vdp, 30, 128),
        sms_argb(BACKDROP),
        "and the ninth is still the one dropped"
    );
}
