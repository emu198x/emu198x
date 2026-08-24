//! Sprite sizes, magnification, and the quadrant layout — from Table 2-5 and
//! Figure 2-21 of the TMS9918A/9928A/9929A data manual (November 1982), held at
//! `reference/by-topic/vdp-tms9918/`.
//!
//! | SIZE (R1 b1) | MAG (R1 b0) | On-screen | Bit→pixel | Bytes/pattern |
//! |---|---|---|---|---|
//! | 0 | 0 | 8x8   | 1:1 | 8  |
//! | 1 | 0 | 16x16 | 1:1 | 32 |
//! | 0 | 1 | 16x16 | 2x2 | 8  |
//! | 1 | 1 | 32x32 | 2x2 | 32 |
//!
//! The two middle rows are the interesting ones: both put a 16x16 square on
//! screen, from different amounts of pattern data. A core that conflated them
//! would pass a test that only measured the square.
//!
//! Eight machines share this chip, so these are eight machines' sprites.

use ti_tms9918::{PALETTE, Tms9918, VdpRegion};

const SAT: u16 = 0x1000;
const SPG: u16 = 0x1800;
const WHITE: u8 = 0x0F;
const SPRITE_X: u8 = 40;
/// Display line = Y + 1, so the sprite's first row is line 60.
const SPRITE_Y: u8 = 59;

fn reg(vdp: &mut Tms9918, index: u8, value: u8) {
    vdp.write_control(value);
    vdp.write_control(0x80 | index);
}

/// One sprite of pattern `name`, with `size16` and `mag` as given.
fn vdp_with(size16: bool, mag: bool, name: u8) -> Tms9918 {
    let mut vdp = Tms9918::new(VdpRegion::Ntsc);
    reg(&mut vdp, 0, 0x00);
    let mut r1 = 0x40; // display enabled
    if size16 {
        r1 |= 0x02;
    }
    if mag {
        r1 |= 0x01;
    }
    reg(&mut vdp, 1, r1);
    reg(&mut vdp, 2, 0x00);
    reg(&mut vdp, 3, 0x80);
    reg(&mut vdp, 4, 0x01);
    #[allow(clippy::cast_possible_truncation)]
    reg(&mut vdp, 5, (SAT / 0x80) as u8);
    #[allow(clippy::cast_possible_truncation)]
    reg(&mut vdp, 6, (SPG / 0x800) as u8);
    reg(&mut vdp, 7, 0x01); // black backdrop

    vdp.write_vram(SAT, SPRITE_Y);
    vdp.write_vram(SAT + 1, SPRITE_X);
    vdp.write_vram(SAT + 2, name);
    vdp.write_vram(SAT + 3, WHITE);
    vdp.write_vram(SAT + 4, 0xD0); // terminator
    vdp
}

fn frame(vdp: &mut Tms9918) {
    for _ in 0..262 {
        vdp.tick_scanline();
    }
}

/// Bounding box of lit sprite pixels, in active-area coordinates.
fn lit_bounds(vdp: &Tms9918) -> Option<(usize, usize, usize, usize)> {
    let width = vdp.framebuffer_width() as usize;
    let left = VdpRegion::Ntsc.border_left() as usize;
    let top = VdpRegion::Ntsc.border_top() as usize;
    let fb = vdp.framebuffer();
    let mut bounds: Option<(usize, usize, usize, usize)> = None;
    for y in 0..192usize {
        for x in 0..256usize {
            if fb[(top + y) * width + left + x] == PALETTE[WHITE as usize] {
                bounds = Some(match bounds {
                    None => (x, x, y, y),
                    Some((x0, x1, y0, y1)) => (x0.min(x), x1.max(x), y0.min(y), y1.max(y)),
                });
            }
        }
    }
    bounds
}

/// Fill `count` bytes of pattern data from the block base with `$FF`.
fn solid(vdp: &mut Tms9918, name: u8, count: u16) {
    let base = SPG + u16::from(name & 0xFC) * 8;
    for i in 0..count {
        vdp.write_vram(base + i, 0xFF);
    }
}

/// Table 2-5, all four rows.
#[test]
fn the_size_and_magnification_matrix() {
    for (size16, mag, expected) in [
        (false, false, 8usize),
        (true, false, 16),
        (false, true, 16),
        (true, true, 32),
    ] {
        let mut vdp = vdp_with(size16, mag, 0);
        solid(&mut vdp, 0, if size16 { 32 } else { 8 });
        frame(&mut vdp);

        let (x0, x1, y0, y1) = lit_bounds(&vdp).unwrap_or_else(|| {
            panic!(
                "SIZE={} MAG={} drew nothing",
                u8::from(size16),
                u8::from(mag)
            )
        });
        let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
        assert_eq!(
            (w, h),
            (expected, expected),
            "SIZE={} MAG={} should put {expected}x{expected} on screen, not {w}x{h}",
            u8::from(size16),
            u8::from(mag)
        );
        assert_eq!(
            (x0, y0),
            (SPRITE_X as usize, SPRITE_Y as usize + 1),
            "and it should start where the attribute puts it (display line = Y + 1)"
        );
    }
}

/// The two 16x16 cases are not the same case.
///
/// Both put a 16x16 square on screen. The magnified one does it from **eight**
/// bytes, so filling only the first eight bytes of the block lights the whole
/// square; the SIZE=1 one needs all thirty-two, so the same eight bytes light
/// only its top-left quadrant.
#[test]
fn magnified_and_size_one_reach_sixteen_pixels_differently() {
    let mut magnified = vdp_with(false, true, 0);
    solid(&mut magnified, 0, 8);
    frame(&mut magnified);
    let (x0, x1, y0, y1) = lit_bounds(&magnified).expect("magnified sprite");
    assert_eq!(
        (x1 - x0 + 1, y1 - y0 + 1),
        (16, 16),
        "eight bytes magnified should fill the whole 16x16"
    );

    let mut size_one = vdp_with(true, false, 0);
    solid(&mut size_one, 0, 8); // only quadrant A
    frame(&mut size_one);
    let (x0, x1, y0, y1) = lit_bounds(&size_one).expect("size-one sprite");
    assert_eq!(
        (x1 - x0 + 1, y1 - y0 + 1),
        (8, 8),
        "the same eight bytes as a SIZE=1 pattern fill one quadrant, not the square"
    );
}

/// Figure 2-21: the quadrants are stored **column-major**.
///
/// Bytes `00-07` are the top-left, `08-0F` the **bottom-left**, `10-17` the
/// **top-right**, `18-1F` the bottom-right. Row-major would swap the middle
/// two, which is the classic way to get this wrong and is invisible on any
/// symmetric sprite.
#[test]
fn a_size_one_pattern_is_four_quadrants_in_column_major_order() {
    // (quadrant byte offset, expected corner as (dx, dy) within the 16x16)
    for (offset, (dx, dy), name) in [
        (0x00u16, (0usize, 0usize), "top-left"),
        (0x08, (0, 8), "bottom-left"),
        (0x10, (8, 0), "top-right"),
        (0x18, (8, 8), "bottom-right"),
    ] {
        let mut vdp = vdp_with(true, false, 0);
        for i in 0..8u16 {
            vdp.write_vram(SPG + offset + i, 0xFF);
        }
        frame(&mut vdp);

        let (x0, x1, y0, y1) = lit_bounds(&vdp)
            .unwrap_or_else(|| panic!("quadrant at ${offset:02X} ({name}) drew nothing"));
        assert_eq!((x1 - x0 + 1, y1 - y0 + 1), (8, 8), "one quadrant is 8x8");
        assert_eq!(
            (x0, y0),
            (SPRITE_X as usize + dx, SPRITE_Y as usize + 1 + dy),
            "bytes ${offset:02X}-${:02X} are the {name} quadrant",
            offset + 7
        );
    }
}

/// Table 3-2: the address is `SPGB + (name & $FC) * 8`, so the low two bits of
/// the name are forced for a SIZE=1 sprite — all four of `$00`, `$01`, `$02`
/// and `$03` select the same 32-byte block.
#[test]
fn the_low_two_name_bits_are_forced_for_a_size_one_sprite() {
    let mut reference = None;
    for name in 0..4u8 {
        let mut vdp = vdp_with(true, false, name);
        solid(&mut vdp, 0, 32);
        frame(&mut vdp);
        let bounds =
            lit_bounds(&vdp).unwrap_or_else(|| panic!("sprite name ${name:02X} drew nothing"));
        match reference {
            None => reference = Some(bounds),
            Some(first) => assert_eq!(
                bounds, first,
                "name ${name:02X} should select the same block as $00"
            ),
        }
    }
}
