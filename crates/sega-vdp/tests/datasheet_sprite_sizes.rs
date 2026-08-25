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
    let index = (REGION.border_top(vdp.active_height()) + line) * REGION.framebuffer_width()
        + REGION.border_left()
        + x;
    vdp.framebuffer()[index as usize]
}

/// The 315-5246's measured CRAM output levels — `%00BBGGRR` through the
/// table in `VdpVariant::output_levels`. Levels 0 and 3 happen to match a
/// bit-replicated expansion, so a test that only uses black and full scale
/// cannot tell the two apart; these helpers use the real table so that
/// coincidence is not load-bearing.
fn sms_argb(entry: u8) -> u32 {
    sms_argb_on(VdpVariant::Sms2, entry)
}

/// The same, for whichever chip is under test. The two drive different output
/// levels — the 315-5124 never reaches full scale — so a test that renders on
/// both cannot share one expected colour.
fn sms_argb_on(variant: VdpVariant, entry: u8) -> u32 {
    let (levels, blue) = match variant {
        VdpVariant::Sms1 => ([0u32, 78, 160, 238], [0u32, 98, 160, 238]),
        VdpVariant::Sms2 => ([0, 89, 174, 255], [0, 89, 174, 255]),
    };
    let e = u32::from(entry) as usize;
    0xFF00_0000 | (levels[e & 3] << 16) | (levels[(e >> 2) & 3] << 8) | blue[(e >> 4) & 3]
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

// ---------------------------------------------------------------------------
// The 315-5124's magnification quirk
// ---------------------------------------------------------------------------

/// Lay `count` magnified 8x8 sprites along one line, 24 pixels apart so a
/// doubled one still cannot touch its neighbour, and measure what each drew.
fn magnified_row(variant: VdpVariant, count: u8) -> SegaVdp {
    let mut vdp = SegaVdp::new(REGION, variant);
    write_register(&mut vdp, 0, 0x04); // Mode 4
    write_register(&mut vdp, 1, 0x41); // display on + MAG
    write_register(&mut vdp, 3, 0xFF); // no 315-5124 masking
    write_register(&mut vdp, 4, 0x07);
    write_register(&mut vdp, 5, 0xFF); // attribute table $3F00
    write_register(&mut vdp, 6, 0x03); // sprite patterns $0000, unmasked
    write_register(&mut vdp, 7, 0x00);
    poke_cram(&mut vdp, 16, BACKDROP);
    poke_cram(&mut vdp, 16 + 5, INK);
    poke_vram(&mut vdp, 0x0020, &solid_tile(5));
    for i in 0..count {
        poke_vram(&mut vdp, 0x3F00 + u16::from(i), &[15]);
        poke_vram(&mut vdp, 0x3F80 + u16::from(i) * 2, &[i * 24, 1]);
    }
    poke_vram(&mut vdp, 0x3F00 + u16::from(count), &[0xD0]);
    vdp
}

fn widths(vdp: &SegaVdp, variant: VdpVariant, count: u8) -> Vec<u32> {
    let ink = sms_argb_on(variant, INK);
    (0..count)
        .map(|i| {
            let start = u32::from(i) * 24;
            (start..start + 24)
                .take_while(|&x| pixel(vdp, 16, x) == ink)
                .count() as u32
        })
        .collect()
}

/// SMS Power, R1 bit 0: "On the SMS1, sprites are always doubled vertically,
/// but only some sprites are doubled horizontally: if N sprites are on a
/// scanline, the first N minus 4 are double-width. Fully works on GG and
/// SMS2."
///
/// So the count that stretches depends on how crowded the line is, and a line
/// of four or fewer gets no horizontal doubling at all. Genesis Plus GX
/// states the same rule from the other end — `if (count < 4) width = 8;` with
/// `count` running down through the sprites in index order.
#[test]
fn the_315_5124_widens_only_the_first_n_minus_4_sprites_on_a_line() {
    for count in 1..=8u8 {
        let mut vdp = magnified_row(VdpVariant::Sms1, count);
        render_frame(&mut vdp);

        let widened = count.saturating_sub(4);
        let expected: Vec<u32> = (0..count)
            .map(|i| if i < widened { 16 } else { 8 })
            .collect();
        assert_eq!(
            widths(&vdp, VdpVariant::Sms1, count),
            expected,
            "with {count} sprites on the line the 315-5124 should widen the first {widened}"
        );
    }
}

/// The 315-5246 has no such rule: every sprite widens however many share the
/// line.
#[test]
fn the_315_5246_widens_every_sprite_on_the_line() {
    for count in 1..=8u8 {
        let mut vdp = magnified_row(VdpVariant::Sms2, count);
        render_frame(&mut vdp);
        assert_eq!(
            widths(&vdp, VdpVariant::Sms2, count),
            vec![16u32; count as usize],
            "the 315-5246 should widen all {count} sprites"
        );
    }
}

/// Only the horizontal half is broken. Vertical doubling works on the
/// 315-5124 for every sprite, including the last four that stay eight pixels
/// wide — so a crowded line of magnified sprites comes out tall and narrow,
/// not unmagnified.
#[test]
fn the_315_5124_still_doubles_every_sprite_vertically() {
    let mut vdp = magnified_row(VdpVariant::Sms1, 8);
    render_frame(&mut vdp);

    let ink = sms_argb_on(VdpVariant::Sms1, INK);
    for i in 0..8u32 {
        let x = i * 24;
        let height = (16..192).take_while(|&y| pixel(&vdp, y, x) == ink).count();
        assert_eq!(
            height, 16,
            "sprite {i} should be sixteen lines tall whether or not it widened"
        );
    }
}

// ---------------------------------------------------------------------------
// Sprites hanging off the top of the screen
// ---------------------------------------------------------------------------

/// One sprite at a chosen Y, on a screen otherwise empty.
fn sprite_at_y(y_raw: u8) -> SegaVdp {
    let mut vdp = SegaVdp::new(REGION, VdpVariant::Sms2);
    write_register(&mut vdp, 0, 0x04);
    write_register(&mut vdp, 1, 0x40);
    write_register(&mut vdp, 3, 0xFF);
    write_register(&mut vdp, 4, 0x07);
    write_register(&mut vdp, 5, 0xFF); // attribute table $3F00
    write_register(&mut vdp, 6, 0x03); // sprite patterns $0000
    write_register(&mut vdp, 7, 0x00);
    poke_cram(&mut vdp, 16, BACKDROP);
    poke_cram(&mut vdp, 16 + 5, INK);
    poke_vram(&mut vdp, 0x0020, &solid_tile(5));
    poke_vram(&mut vdp, 0x3F00, &[y_raw, 0xD0]);
    poke_vram(&mut vdp, 0x3F80, &[0, 1]);
    vdp
}

/// A Y at the bottom of the byte's range puts a sprite off the *top* of the
/// screen, part-way in. Without the wrap a sprite scrolling on from above
/// pops into existence all at once as its coordinate crosses zero, instead of
/// sliding in a line at a time.
///
/// MAME: "wrap from top if y position is >= 240".
#[test]
fn a_y_at_the_end_of_the_range_hangs_the_sprite_off_the_top() {
    // $FA is 250: first line -5, so three of the sprite's eight rows show.
    let mut vdp = sprite_at_y(0xFA);
    render_frame(&mut vdp);
    let ink = sms_argb(INK);
    assert_eq!(
        pixel(&vdp, 0, 0),
        ink,
        "the sprite's sixth row is on line 0"
    );
    assert_eq!(pixel(&vdp, 2, 0), ink, "and its last on line 2");
    assert_eq!(
        pixel(&vdp, 3, 0),
        sms_argb(BACKDROP),
        "an 8x8 sprite starting at -5 ends after line 2"
    );
}

/// The visible edge of the wrap. An 8x8 sprite whose first line is -8 is
/// entirely above the picture; one line lower and its last row lands on line
/// 0. Without the wrap both are far below the picture instead, so this is the
/// boundary the wrap creates rather than one the coordinate already had.
///
/// Where exactly the wrap *begins* is not observable here, and that is worth
/// knowing rather than glossing: a sprite with Y 240 wraps to first line -15
/// and an 8x8 one is still wholly above the screen, so every value from 209
/// to 247 looks the same — invisible — whether it wrapped or not. The two
/// emulators' thresholds disagree across that band and the disagreement
/// cannot be seen. It becomes visible only in the 240-line mode, where MAME
/// wraps and Genesis Plus GX does not; we take MAME's, which is stated
/// without qualification where the other flags itself as unverified.
#[test]
fn the_wrap_shows_a_sprite_that_would_otherwise_be_below_the_picture() {
    let mut clear = sprite_at_y(247); // first line -8
    render_frame(&mut clear);
    assert_eq!(
        pixel(&clear, 0, 0),
        sms_argb(BACKDROP),
        "eight rows starting at -8 all fall above the picture"
    );

    let mut showing = sprite_at_y(248); // first line -7
    render_frame(&mut showing);
    assert_eq!(
        pixel(&showing, 0, 0),
        sms_argb(INK),
        "one line lower and the sprite's last row is on line 0"
    );
    assert_eq!(
        pixel(&showing, 1, 0),
        sms_argb(BACKDROP),
        "and only that row"
    );
}

/// A wrapped sprite still takes one of the eight slots on the lines it
/// covers, and still draws from the right row of its pattern rather than
/// from row zero.
#[test]
fn a_wrapped_sprite_draws_its_lower_rows() {
    // A pattern whose rows differ, so which row is drawn is visible.
    let mut vdp = sprite_at_y(0xFA); // first line -5
    let mut tile = [0u8; 32];
    for row in 0..8 {
        // Row n takes colour n + 1.
        let colour = (row + 1) as u8;
        for (plane, byte) in tile[row * 4..row * 4 + 4].iter_mut().enumerate() {
            *byte = if colour & (1 << plane) != 0 {
                0xFF
            } else {
                0x00
            };
        }
    }
    poke_vram(&mut vdp, 0x0020, &tile);
    for colour in 1..=8u8 {
        poke_cram(&mut vdp, 16 + colour, colour * 4);
    }
    render_frame(&mut vdp);

    // Line 0 is the sprite's row 5, since it began five lines above zero.
    for (line, row) in [(0u32, 5u8), (1, 6), (2, 7)] {
        assert_eq!(
            pixel(&vdp, line, 0),
            sms_argb((row + 1) * 4),
            "line {line} should draw the pattern's row {row}"
        );
    }
}

/// In the 240-line mode a Y in the 200s is an ordinary position near the
/// bottom of the picture, and must not be read as a wrapped one.
///
/// This is where the wrap's threshold stops being academic. In the 192-line
/// mode everything from 209 to 247 is invisible either way, so a threshold
/// set too low costs nothing; here it hides a sprite that belongs on screen.
/// Both reference emulators agree on this case — MAME wraps from 240 and
/// Genesis Plus GX from past the end of a 240-line display — so it is the
/// low threshold, not the choice between them, that this rules out.
#[test]
fn a_tall_mode_shows_sprites_low_down_rather_than_wrapping_them() {
    let mut vdp = SegaVdp::new(REGION, VdpVariant::Sms2);
    write_register(&mut vdp, 0, 0x06); // Mode 4 + M2
    write_register(&mut vdp, 1, 0x48); // display on + M3: 240 lines
    write_register(&mut vdp, 3, 0xFF);
    write_register(&mut vdp, 4, 0x07);
    write_register(&mut vdp, 5, 0xFF);
    write_register(&mut vdp, 6, 0x03);
    write_register(&mut vdp, 7, 0x00);
    poke_cram(&mut vdp, 16, BACKDROP);
    poke_cram(&mut vdp, 16 + 5, INK);
    poke_vram(&mut vdp, 0x0020, &solid_tile(5));
    // Sprite 0 at Y 230, so lines 231 to 238. The second entry is a Y that
    // wraps clear of the picture, since $D0 is not a terminator up here.
    poke_vram(&mut vdp, 0x3F00, &[230, 245]);
    poke_vram(&mut vdp, 0x3F80, &[0, 1, 0, 1]);

    while !vdp.tick_scanline() {}
    for _ in 0..240 {
        vdp.tick_scanline();
    }

    assert_eq!(
        pixel(&vdp, 231, 0),
        sms_argb(INK),
        "Y 230 is a position, not a wrap"
    );
    assert_eq!(pixel(&vdp, 238, 0), sms_argb(INK), "and it is eight tall");
    assert_eq!(pixel(&vdp, 239, 0), sms_argb(BACKDROP));
}
