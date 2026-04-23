//! Phase 1 characterisation — sprites and collision detection.
//!
//! Covers task #153 from wiki/amiga/denise-ocs-porting-gap-list.md.
//!
//! HRM references:
//!   - HRM 4-1  "Hardware Sprites"
//!   - HRM Fig. 4-13 "Sprite Control Register Coding" — arming rules
//!   - HRM 4-5  attached-pair sprites (CTL bit 7 = ATTACH)
//!   - HRM 3-21 sprite-vs-playfield priority (BPLCON2)
//!   - HRM Table 3-10 collision register (CLXDAT) bit layout
//!
//! Sprite position encoding (HRM Fig. 4-12):
//!   SPRxPOS high byte = vstart bits 7-0
//!   SPRxPOS low byte  = hstart bits 8-1
//!   SPRxCTL high byte = vstop  bits 7-0
//!   SPRxCTL bit 2     = vstart bit 8
//!   SPRxCTL bit 1     = vstop  bit 8
//!   SPRxCTL bit 0     = hstart bit 0
//!   SPRxCTL bit 7     = attach (odd sprite only)
//!
//! Archive encodes this via `write_sprite_pos` / `write_sprite_ctl`.

use commodore_denise_ocs::DeniseOcs;

fn encode_sprite_pos_ctl(hstart: u16, vstart: u16, vstop: u16) -> (u16, u16) {
    let pos = ((vstart & 0x00FF) << 8) | ((hstart >> 1) & 0x00FF);
    let ctl = ((vstop & 0x00FF) << 8)
        | (((vstart >> 8) & 1) << 2)
        | (((vstop >> 8) & 1) << 1)
        | (hstart & 1);
    (pos, ctl)
}

/// Arrange a minimal playfield: plane 0 cleared, BPU=1, palette
/// programmed with COLOR00=black and sprite bank colours. Leaves
/// Denise ready for sprite-overlay tests at any beam position.
fn with_clear_playfield() -> DeniseOcs {
    let mut d = DeniseOcs::new();
    d.bplcon0 = 0x1000; // BPU=1 lores
    d.set_palette(0, 0x000);
    d.begin_beam_line();
    d.bpl_data[0] = 0x0000;
    d.trigger_shift_load();
    d
}

#[test]
fn sprite_data_arms_comparator_ctl_disarms() {
    // HRM Fig. 4-13: writing SPRxCTL clears the comparator; writing
    // SPRxDATA rearms it; writing SPRxDATB alone is neutral.
    let mut d = with_clear_playfield();
    d.set_palette(17, 0xF00); // sprite 0/1 bank, colour code 1

    let (pos, ctl) = encode_sprite_pos_ctl(30, 10, 11);
    d.write_sprite_pos(0, pos);
    d.write_sprite_ctl(0, ctl); // disarm
    d.write_sprite_datb(0, 0x0000);
    d.write_sprite_data(0, 0x8000); // arm + load

    assert_eq!(
        d.output_pixel_color(30, 10),
        DeniseOcs::rgb12_to_argb32(0xF00),
        "DATA write should arm comparator and display sprite"
    );

    d.write_sprite_ctl(0, ctl); // disarm again
    assert_eq!(
        d.output_pixel_color(30, 10),
        DeniseOcs::rgb12_to_argb32(0x000),
        "CTL write should suppress sprite until re-armed"
    );

    d.write_sprite_datb(0, 0x0000); // neutral
    assert_eq!(
        d.output_pixel_color(30, 10),
        DeniseOcs::rgb12_to_argb32(0x000),
        "DATB alone should not rearm"
    );
}

#[test]
fn sprite_hstart_comparator_honours_subpixel_bit() {
    // HRM 4-3: hstart is 9 bits — SPRxPOS low byte << 1 | SPRxCTL bit
    // 0. The low bit shifts the comparator by one beam step.
    let mut d = with_clear_playfield();
    d.set_palette(17, 0xF00);
    let (pos, ctl) = encode_sprite_pos_ctl(41, 10, 11); // odd hstart
    d.write_sprite_pos(0, pos);
    d.write_sprite_ctl(0, ctl);
    d.write_sprite_datb(0, 0x0000);
    d.write_sprite_data(0, 0x8000);

    assert_eq!(
        d.output_pixel_color(40, 10),
        DeniseOcs::rgb12_to_argb32(0x000),
        "beam just before hstart should be background"
    );
    assert_eq!(
        d.output_pixel_color(41, 10),
        DeniseOcs::rgb12_to_argb32(0xF00),
        "sprite should light at odd hstart"
    );
}

#[test]
fn sprite_vstart_vstop_gate_vertical_extent() {
    // HRM 4-3: sprite is active on lines [vstart, vstop). Outside
    // that range the shifter is idle and COLOR00 shows.
    let mut d = with_clear_playfield();
    d.set_palette(17, 0xF00);
    let (pos, ctl) = encode_sprite_pos_ctl(30, 20, 22); // lines 20, 21
    d.write_sprite_pos(0, pos);
    d.write_sprite_ctl(0, ctl);
    d.write_sprite_datb(0, 0x0000);
    d.write_sprite_data(0, 0x8000);

    assert_eq!(
        d.output_pixel_color(30, 19),
        DeniseOcs::rgb12_to_argb32(0x000),
        "before vstart -> no sprite"
    );
    assert_eq!(
        d.output_pixel_color(30, 20),
        DeniseOcs::rgb12_to_argb32(0xF00),
        "on vstart -> sprite visible"
    );
    assert_eq!(
        d.output_pixel_color(30, 21),
        DeniseOcs::rgb12_to_argb32(0xF00),
        "between vstart and vstop -> sprite visible"
    );
    assert_eq!(
        d.output_pixel_color(30, 22),
        DeniseOcs::rgb12_to_argb32(0x000),
        "on vstop -> sprite no longer visible"
    );
}

#[test]
fn transparent_sprite_pixel_leaves_playfield_visible() {
    // Sprite colour code 00 is transparent. A sprite armed at a beam
    // position with both DATA and DATB = 0 must not suppress the
    // playfield underneath.
    let mut d = DeniseOcs::new();
    d.bplcon0 = 0x1000;
    d.set_palette(0, 0x000);
    d.set_palette(1, 0x0F0);
    d.set_palette(17, 0xF00);
    d.begin_beam_line();
    // Playfield bit at source pixel 5 (MSB is pixel 0).
    d.bpl_data[0] = 1 << (15 - 5);
    d.trigger_shift_load();

    // Sprite at hstart=5 armed with DATA=DATB=0.
    let (pos, ctl) = encode_sprite_pos_ctl(5, 0, 312);
    d.write_sprite_pos(0, pos);
    d.write_sprite_ctl(0, ctl);
    d.write_sprite_datb(0, 0x0000);
    d.write_sprite_data(0, 0x0000); // arms but every pixel transparent

    for x in 0..5 {
        let _ = d.output_pixel_with_beam(x, 0, x, 0);
    }
    let dbg = d.output_pixel_with_beam(5, 0, 5, 0);
    assert_eq!(
        dbg.final_color_idx, 1,
        "transparent sprite pixel -> playfield remains visible"
    );
}

#[test]
fn lower_numbered_sprite_wins_on_overlap() {
    // HRM 4-3: when multiple sprites output non-transparent pixels at
    // the same beam position, the lower-numbered sprite (lower pair
    // group) wins.
    let mut d = with_clear_playfield();
    d.set_palette(17, 0xF00); // sprite 0 pair, code 1 -> COLOR17
    d.set_palette(21, 0x0F0); // sprite 2 pair, code 1 -> COLOR21

    let (pos0, ctl0) = encode_sprite_pos_ctl(20, 5, 6);
    d.write_sprite_pos(0, pos0);
    d.write_sprite_ctl(0, ctl0);
    d.write_sprite_datb(0, 0x0000);
    d.write_sprite_data(0, 0x8000);

    let (pos2, ctl2) = encode_sprite_pos_ctl(20, 5, 6);
    d.write_sprite_pos(2, pos2);
    d.write_sprite_ctl(2, ctl2);
    d.write_sprite_datb(2, 0x0000);
    d.write_sprite_data(2, 0x8000);

    assert_eq!(
        d.output_pixel_color(20, 5),
        DeniseOcs::rgb12_to_argb32(0xF00),
        "sprite 0 (lower number) should win over sprite 2",
    );
}

#[test]
fn attached_pair_produces_4_bit_colour_from_combined_codes() {
    // HRM 4-5: odd sprite CTL bit 7 = ATTACH. Pair produces 4-bit
    // colour index where even sprite supplies low 2 bits and odd
    // sprite supplies high 2 bits.
    //
    // Build sprite 0 (even) with colour code 01, sprite 1 (odd) with
    // colour code 10 and ATTACH. Combined code = 10_01 = 9, mapped
    // into the 16-entry sprite palette starting at index 16.
    let mut d = with_clear_playfield();
    d.set_palette(16 + 9, 0x0FF); // target colour for code 9

    // Sprite 0: code 01 means DATA bit = 1, DATB bit = 0 at the MSB.
    let (pos0, ctl0) = encode_sprite_pos_ctl(30, 5, 6);
    d.write_sprite_pos(0, pos0);
    d.write_sprite_ctl(0, ctl0);
    d.write_sprite_datb(0, 0x0000);
    d.write_sprite_data(0, 0x8000);

    // Sprite 1 (odd): code 10 means DATA bit = 0, DATB bit = 1 at MSB.
    // CTL bit 7 set for ATTACH.
    let (pos1, ctl1) = encode_sprite_pos_ctl(30, 5, 6);
    d.write_sprite_pos(1, pos1);
    d.write_sprite_ctl(1, ctl1 | 0x0080);
    d.write_sprite_data(1, 0x0000);
    d.write_sprite_datb(1, 0x8000);

    assert_eq!(
        d.output_pixel_color(30, 5),
        DeniseOcs::rgb12_to_argb32(0x0FF),
        "attached pair should output 4-bit colour 9",
    );
}

#[test]
fn sprite_vs_pf1_priority_via_bplcon2_pf1p_field() {
    // BPLCON2 bits 2:0 = PF1P (sprite group threshold for PF1).
    // Archive rule: a sprite group wins over PF1 only if
    // `sprite_group < PF1P`. So PF1P=1 -> only group 0 wins;
    // PF1P=0 -> no sprite wins over PF1.
    //
    // Set up PF1 at the same beam as sprite 0 and flip PF1P.
    let build = || {
        let mut d = DeniseOcs::new();
        d.bplcon0 = 0x1000;
        d.set_palette(0, 0x000);
        d.set_palette(1, 0x00F); // PF1 colour
        d.set_palette(17, 0xF00); // sprite colour
        d.begin_beam_line();
        d.bpl_data[0] = 0x8000; // PF1 lit at pixel 0
        d.trigger_shift_load();
        let (pos, ctl) = encode_sprite_pos_ctl(0, 5, 6);
        d.write_sprite_pos(0, pos);
        d.write_sprite_ctl(0, ctl);
        d.write_sprite_datb(0, 0x0000);
        d.write_sprite_data(0, 0x8000);
        d
    };

    let mut d = build();
    d.bplcon2 = 0x0000; // PF1P = 0 -> no sprite in front of PF1
    assert_eq!(
        d.output_pixel_color(0, 5),
        DeniseOcs::rgb12_to_argb32(0x00F),
        "PF1P=0 -> PF1 wins over sprite group 0"
    );

    let mut d = build();
    d.bplcon2 = 0x0001; // PF1P = 1 -> sprite group 0 wins over PF1
    assert_eq!(
        d.output_pixel_color(0, 5),
        DeniseOcs::rgb12_to_argb32(0xF00),
        "PF1P=1 -> sprite group 0 in front of PF1"
    );
}

#[test]
fn clxdat_is_read_and_clear() {
    // HRM 3-10: reading CLXDAT returns the accumulated collision
    // bits and clears them. A second read returns 0 unless new
    // collisions have been latched.
    //
    // Setup: BPU=2, enable plane-1 and plane-2 in the collision
    // comparator with MVBP1=MVBP2=1. Drive both planes high at the
    // same pixel so odd_match && even_match -> CLXDAT bit 0 sets.
    let mut d = DeniseOcs::new();
    d.bplcon0 = 0x2000;
    d.clxcon = (1 << 7) | (1 << 6) | (1 << 1) | (1 << 0); // ENBP1/2 + MVBP1/2
    d.set_palette(0, 0x000);
    d.set_palette(3, 0x0F0);

    d.begin_beam_line();
    d.bpl_data[0] = 0x8000;
    d.bpl_data[1] = 0x8000;
    d.trigger_shift_load();
    let _ = d.output_pixel_with_beam(0, 0, 0, 0);

    let first = d.read_clxdat();
    let second = d.read_clxdat();
    assert_ne!(
        first & 1,
        0,
        "CLXDAT bit 0 should latch BP-odd/BP-even match"
    );
    assert_eq!(second, 0, "subsequent read returns 0 (read-clear)");
}

#[test]
fn clxcon_disables_bitplane_match_per_plane() {
    // CLXCON bits 11-6 = ENBP6..ENBP1 (enable match for that plane).
    // If a plane's enable bit is clear, that plane's state is
    // ignored in the collision comparator.
    //
    // Setup: 2 planes, PF1 = 1 (plane 0 set), PF2 = 0 (plane 1 clear).
    // With ENBP1+ENBP2 enabled and MVBP1=1, MVBP2=0 the match
    // succeeds -> collision bit 0 (BP-odd/BP-even) set.
    let mut d = DeniseOcs::new();
    d.bplcon0 = 0x2000;
    d.set_palette(0, 0x000);
    d.set_palette(1, 0x00F);

    // ENBP1 (bit 6) + ENBP2 (bit 7) + MVBP1 (bit 0); MVBP2=0.
    d.clxcon = (1 << 7) | (1 << 6) | 1;

    d.begin_beam_line();
    d.bpl_data[0] = 0x8000; // plane 0 set
    d.bpl_data[1] = 0x0000; // plane 1 clear
    d.trigger_shift_load();
    let _ = d.output_pixel_with_beam(0, 0, 0, 0);
    assert_ne!(
        d.read_clxdat() & 1,
        0,
        "MVBP1=1, MVBP2=0 matches the plane bits -> collision bit 0 set"
    );

    // Clear ENBP1 -> plane 0 is ignored, match succeeds even when
    // the plane bits don't match the MVBP value.
    let mut d = DeniseOcs::new();
    d.bplcon0 = 0x2000;
    d.clxcon = 1 << 7; // ENBP1 clear, MVBP1/MVBP2=0

    d.begin_beam_line();
    d.bpl_data[0] = 0x8000; // plane 0 set
    d.bpl_data[1] = 0x0000;
    d.trigger_shift_load();
    let _ = d.output_pixel_with_beam(0, 0, 0, 0);
    assert_ne!(
        d.read_clxdat() & 1,
        0,
        "ENBP1=0 masks plane 0 out of the comparator"
    );
}

#[test]
fn clxdat_latches_sprite_pair_crosses() {
    // HRM Table 3-10 bits 9-14 = sprite-pair-cross collisions.
    // Bit 9 = SP01 ^ SP23 (sprite groups 0 and 1 both active at the
    // same pixel).
    let mut d = with_clear_playfield();
    d.clxcon = 0xFFFF; // enable everything

    // Sprite 0 (group 0) and sprite 2 (group 1) armed at the same beam.
    let (pos0, ctl0) = encode_sprite_pos_ctl(30, 5, 6);
    d.write_sprite_pos(0, pos0);
    d.write_sprite_ctl(0, ctl0);
    d.write_sprite_datb(0, 0x0000);
    d.write_sprite_data(0, 0x8000);

    let (pos2, ctl2) = encode_sprite_pos_ctl(30, 5, 6);
    d.write_sprite_pos(2, pos2);
    d.write_sprite_ctl(2, ctl2);
    d.write_sprite_datb(2, 0x0000);
    d.write_sprite_data(2, 0x8000);

    let _ = d.output_pixel_with_beam(30, 5, 30, 5);
    let clx = d.read_clxdat();
    assert_ne!(
        clx & (1 << 9),
        0,
        "sprite pair 0 + sprite pair 1 at same pixel -> bit 9 set"
    );
}
