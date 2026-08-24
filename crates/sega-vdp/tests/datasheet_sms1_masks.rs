//! The 315-5124's address-bus mask bits.
//!
//! SMS Power's VDP Registers page describes the difference between the two
//! Master System VDPs as a logic gate: "Conceptually you can think of the bit
//! being an input to a logic gate where the other input is a VRAM address bus
//! bit. On the SMS1 VDP, this gate is like an AND; If the bit is set to 1, the
//! output follows the input. Otherwise the output is forced to 0 at all times
//! [...] On the SMS2 VDP, the gate always gets a 1 from the register, so no
//! addresses get masked off."
//!
//! Five registers carry such bits. Every test here runs the same scene on both
//! chips: the 315-5124 must mask, the 315-5246 must not.
//!
//! Genesis Plus GX implements all five and its masks agree with the bus
//! diagrams on that page — `nt_mask`, `st_mask` and `sg_mask` in
//! `vdp_render.c`, and the `reg[3] << 1` / `(reg[4] & 0x07) << 6` pair in
//! `render_bg_m4` that splits the background tile index between bitplanes.

use sega_vdp::{SegaVdp, VdpRegion, VdpVariant};

const REGION: VdpRegion = VdpRegion::Ntsc;
const BACKDROP: u8 = 0x30;

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

/// A tile whose four bitplanes are set independently, so a fetch that takes
/// planes 0-1 from one tile and planes 2-3 from another shows it.
fn tile_from_planes(planes: [bool; 4]) -> [u8; 32] {
    let row = planes.map(|on| if on { 0xFFu8 } else { 0x00 });
    let mut tile = [0u8; 32];
    for r in 0..8 {
        tile[r * 4..r * 4 + 4].copy_from_slice(&row);
    }
    tile
}

fn solid_tile(colour: u8) -> [u8; 32] {
    tile_from_planes([
        colour & 1 != 0,
        colour & 2 != 0,
        colour & 4 != 0,
        colour & 8 != 0,
    ])
}

fn render_frame(vdp: &mut SegaVdp) {
    while !vdp.tick_scanline() {}
    for _ in 0..192 {
        vdp.tick_scanline();
    }
}

fn pixel(vdp: &SegaVdp, line: u32, x: u32) -> u32 {
    let index =
        (REGION.border_top(192) + line) * REGION.framebuffer_width() + REGION.border_left() + x;
    vdp.framebuffer()[index as usize]
}

fn sms_argb(entry: u8) -> u32 {
    let level = |c: u32| c * 85;
    0xFF00_0000
        | (level(u32::from(entry) & 3) << 16)
        | (level((u32::from(entry) >> 2) & 3) << 8)
        | level((u32::from(entry) >> 4) & 3)
}

fn blank(variant: VdpVariant) -> SegaVdp {
    let mut vdp = SegaVdp::new(REGION, variant);
    write_register(&mut vdp, 0, 0x04); // Mode 4
    write_register(&mut vdp, 1, 0x40); // display on
    // "For Mode 4 on the SMS1 VDP, all bits should be 1 to give normal
    // operation" (R3), and "at least the low three bits should be 1" (R4).
    // A 315-5124 left at reset masks its background tile index down to
    // nothing, so this is the init every game writes.
    write_register(&mut vdp, 3, 0xFF);
    write_register(&mut vdp, 4, 0x07);
    write_register(&mut vdp, 7, 0x00); // backdrop = CRAM entry 16
    poke_cram(&mut vdp, 16, BACKDROP);
    vdp
}

/// Run the same scene on both chips and return what each drew at one pixel.
fn both<F: Fn(&mut SegaVdp)>(line: u32, x: u32, build: F) -> (u32, u32) {
    let mut sms1 = blank(VdpVariant::Sms1);
    build(&mut sms1);
    render_frame(&mut sms1);

    let mut sms2 = blank(VdpVariant::Sms2);
    build(&mut sms2);
    render_frame(&mut sms2);

    (pixel(&sms1, line, x), pixel(&sms2, line, x))
}

// ---------------------------------------------------------------------------
// R2 bit 0 — the name table's row bit
// ---------------------------------------------------------------------------

/// "The mask bit is ANDed with the high bit of the y coordinate. This leads to
/// tilemap mirroring with the SMS1 VDP."
///
/// Row 16 is the first row whose number has bit 4 set, which is address bit
/// 10. Force that bit to 0 and row 16 fetches row 0 instead.
#[test]
fn clearing_register_2_bit_0_mirrors_the_bottom_half_of_the_tilemap() {
    let scene = |reg2: u8| {
        move |vdp: &mut SegaVdp| {
            write_register(vdp, 2, reg2);
            poke_cram(vdp, 1, 0x03); // red
            poke_cram(vdp, 2, 0x0C); // green
            poke_vram(vdp, 0x0020, &solid_tile(1));
            poke_vram(vdp, 0x0040, &solid_tile(2));
            for col in 0..32u16 {
                poke_vram(vdp, 0x3800 + col * 2, &[0x01, 0x00]); // row 0
                poke_vram(vdp, 0x3800 + 16 * 64 + col * 2, &[0x02, 0x00]); // row 16
            }
        }
    };

    // Line 128 is row 16. With the mask bit set, both chips draw row 16.
    let (sms1, sms2) = both(128, 0, scene(0x0F));
    assert_eq!(
        sms1,
        sms_argb(0x0C),
        "mask bit set: the 315-5124 reads row 16"
    );
    assert_eq!(sms2, sms_argb(0x0C), "the 315-5246 reads row 16");

    // With it clear, only the 315-5124 folds row 16 onto row 0.
    let (sms1, sms2) = both(128, 0, scene(0x0E));
    assert_eq!(
        sms1,
        sms_argb(0x03),
        "mask bit clear: the 315-5124 mirrors row 0 into row 16"
    );
    assert_eq!(
        sms2,
        sms_argb(0x0C),
        "the 315-5246 has no gate to close and still reads row 16"
    );
}

// ---------------------------------------------------------------------------
// R3 and R4 — the background tile index, split between bitplanes
// ---------------------------------------------------------------------------

/// "Color Table Base Address (resp. Pattern Generator Table Base Address)
/// register bits 7:0 (resp. bits 2:0) are used as a mask on tile index upper
/// bits when fetching bitplanes 0&1 (resp. bitplanes 2&3)."
///
/// So a single pixel's four-bit colour can be assembled out of two different
/// tiles. Tile 2 is fully lit and tile 0 is blank, so masking index bit 1 out
/// of one half of the fetch leaves that half reading zeroes.
#[test]
fn register_3_masks_the_tile_index_for_the_low_bitplanes() {
    let scene = |reg3: u8| {
        move |vdp: &mut SegaVdp| {
            write_register(vdp, 2, 0x0F); // name table $3800, row bit unmasked
            write_register(vdp, 3, reg3);
            write_register(vdp, 4, 0x07); // no mask on the high bitplanes
            for entry in 0..16u8 {
                poke_cram(vdp, entry, entry);
            }
            poke_vram(vdp, 0x0000, &tile_from_planes([false; 4])); // tile 0 blank
            poke_vram(vdp, 0x0040, &tile_from_planes([true; 4])); // tile 2 lit
            poke_vram(vdp, 0x3800, &[0x02, 0x00]); // draw tile 2
        }
    };

    let (sms1, sms2) = both(0, 0, scene(0xFF));
    assert_eq!(
        sms1,
        sms_argb(15),
        "unmasked, all four planes come from tile 2"
    );
    assert_eq!(sms2, sms_argb(15));

    // R3 bit 0 gates tile-index bit 1, so clearing it sends the low bitplanes
    // to tile 0 while the high pair still reads tile 2.
    let (sms1, sms2) = both(0, 0, scene(0xFE));
    assert_eq!(
        sms1,
        sms_argb(0b1100),
        "the 315-5124 takes planes 0-1 from tile 0 and planes 2-3 from tile 2"
    );
    assert_eq!(sms2, sms_argb(15), "the 315-5246 masks nothing");
}

/// R4's low three bits do the same job for bitplanes 2 and 3, but reach only
/// index bits 8-6 — so it takes a tile number above 63 to show it.
#[test]
fn register_4_masks_the_tile_index_for_the_high_bitplanes() {
    let scene = |reg4: u8| {
        move |vdp: &mut SegaVdp| {
            write_register(vdp, 2, 0x0F);
            write_register(vdp, 3, 0xFF); // no mask on the low bitplanes
            write_register(vdp, 4, reg4);
            for entry in 0..16u8 {
                poke_cram(vdp, entry, entry);
            }
            poke_vram(vdp, 0x0000, &tile_from_planes([false; 4])); // tile 0 blank
            poke_vram(vdp, 64 * 32, &tile_from_planes([true; 4])); // tile 64 lit
            poke_vram(vdp, 0x3800, &[64, 0x00]); // draw tile 64
        }
    };

    let (sms1, sms2) = both(0, 0, scene(0x07));
    assert_eq!(
        sms1,
        sms_argb(15),
        "unmasked, all four planes come from tile 64"
    );
    assert_eq!(sms2, sms_argb(15));

    // R4 bit 0 gates tile-index bit 6.
    let (sms1, sms2) = both(0, 0, scene(0x06));
    assert_eq!(
        sms1,
        sms_argb(0b0011),
        "the 315-5124 takes planes 2-3 from tile 0 and planes 0-1 from tile 64"
    );
    assert_eq!(sms2, sms_argb(15), "the 315-5246 masks nothing");
}

// ---------------------------------------------------------------------------
// R5 bit 0 — the attribute table's second half
// ---------------------------------------------------------------------------

/// "The mask bit is ANDed with the address; thus, the x-coordinates and tile
/// numbers will incorrectly map to the first half of the sprite attribute
/// table if the mask bit is 0."
///
/// The Y coordinates live at base+0..63 and the X/pattern pairs at base+$80
/// upwards, so the gate is on address bit 7. Close it and a sprite reads its
/// X out of its own Y byte and its pattern out of the next sprite's.
#[test]
fn clearing_register_5_bit_0_folds_sprite_x_and_pattern_into_the_y_half() {
    let scene = |reg5: u8| {
        move |vdp: &mut SegaVdp| {
            write_register(vdp, 5, reg5); // attribute table $3F00 either way
            write_register(vdp, 6, 0x03); // sprite patterns $0000, unmasked
            poke_cram(vdp, 16 + 5, 0x03); // red
            poke_vram(vdp, 0x0020, &solid_tile(5)); // pattern 1
            poke_vram(vdp, 0xD0 * 32, &solid_tile(5)); // pattern $D0
            // Y bytes: sprite 0 on line 8, then the list terminator. Read as
            // X and pattern instead, they say "x = 7, pattern $D0".
            poke_vram(vdp, 0x3F00, &[7, 0xD0]);
            poke_vram(vdp, 0x3F80, &[100, 1]); // the real x = 100, pattern 1
        }
    };

    let (sms1, sms2) = both(8, 100, scene(0xFF));
    assert_eq!(
        sms1,
        sms_argb(0x03),
        "mask bit set: the sprite is at x = 100"
    );
    assert_eq!(sms2, sms_argb(0x03));

    let (sms1, sms2) = both(8, 100, scene(0xFE));
    assert_eq!(
        sms1,
        sms_argb(BACKDROP),
        "mask bit clear: the 315-5124 no longer finds a sprite at x = 100"
    );
    assert_eq!(sms2, sms_argb(0x03), "the 315-5246 masks nothing");

    // It moved to where the Y bytes said, rather than vanishing.
    let (sms1, _) = both(8, 7, scene(0xFE));
    assert_eq!(
        sms1,
        sms_argb(0x03),
        "the 315-5124 read x out of the Y half and put the sprite at 7"
    );
}

// ---------------------------------------------------------------------------
// R6 bits 1-0 — the sprite tile number
// ---------------------------------------------------------------------------

/// "The mask bits are ANDed with the address, which will have the effect of
/// restricting effective tile set from 255 down to 128 or 64 tiles."
///
/// They gate tile-number bits 7 and 6, so tile 64 folds onto tile 0 when bit
/// 0 is clear.
#[test]
fn clearing_register_6_low_bits_shrinks_the_sprite_tile_set() {
    let scene = |reg6: u8| {
        move |vdp: &mut SegaVdp| {
            write_register(vdp, 5, 0xFF);
            write_register(vdp, 6, reg6); // patterns at $0000
            poke_cram(vdp, 16 + 5, 0x03); // red
            poke_cram(vdp, 16 + 6, 0x0C); // green
            poke_vram(vdp, 0x0000, &solid_tile(6)); // tile 0
            poke_vram(vdp, 64 * 32, &solid_tile(5)); // tile 64
            poke_vram(vdp, 0x3F00, &[7, 0xD0]);
            poke_vram(vdp, 0x3F80, &[0, 64]); // x = 0, pattern 64
        }
    };

    let (sms1, sms2) = both(8, 0, scene(0x03));
    assert_eq!(sms1, sms_argb(0x03), "unmasked, the sprite uses tile 64");
    assert_eq!(sms2, sms_argb(0x03));

    let (sms1, sms2) = both(8, 0, scene(0x02));
    assert_eq!(
        sms1,
        sms_argb(0x0C),
        "the 315-5124 folds tile 64 onto tile 0 with bit 0 clear"
    );
    assert_eq!(sms2, sms_argb(0x03), "the 315-5246 masks nothing");
}

/// The whole point of the mask bits is that they are invisible in ordinary
/// use: a BIOS-style init sets every one of them, and then the two chips draw
/// the same picture. This is the guard against a mask being applied when it
/// should not be.
#[test]
fn with_every_mask_bit_set_the_two_chips_agree() {
    let scene = |vdp: &mut SegaVdp| {
        write_register(vdp, 2, 0xFF);
        write_register(vdp, 3, 0xFF);
        write_register(vdp, 4, 0xFF);
        write_register(vdp, 5, 0xFF);
        write_register(vdp, 6, 0xFB); // patterns at $0000, mask bits set
        for entry in 0..16u8 {
            poke_cram(vdp, entry, entry);
            poke_cram(vdp, 16 + entry, 0x3F - entry);
        }
        poke_vram(vdp, 0x0040, &solid_tile(9)); // background tile 2
        poke_vram(vdp, 64 * 32, &solid_tile(5)); // sprite tile 64
        for row in 0..24u16 {
            for col in 0..32u16 {
                poke_vram(vdp, 0x3800 + row * 64 + col * 2, &[0x02, 0x00]);
            }
        }
        poke_vram(vdp, 0x3F00, &[7, 0xD0]);
        poke_vram(vdp, 0x3F80, &[100, 64]);
    };

    let mut sms1 = blank(VdpVariant::Sms1);
    scene(&mut sms1);
    render_frame(&mut sms1);
    let mut sms2 = blank(VdpVariant::Sms2);
    scene(&mut sms2);
    render_frame(&mut sms2);

    assert_eq!(
        sms1.framebuffer(),
        sms2.framebuffer(),
        "with no gate closed the two chips must draw an identical frame"
    );
}
