//! Phase 1 characterisation — sprites and collision detection.
//!
//! Covers task #153 from knowledge/amiga/denise-ocs-porting-gap-list.md.
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
fn manual_sprite_data_repeats_on_every_line_until_ctl_disarms() {
    // HRM "Manual Mode": VSTART/VSTOP belong to Agnus's DMA lifecycle.
    // Once SPRxDATA arms Denise, unchanged data is displayed on every
    // line at HSTART until a SPRxCTL write disarms it.
    let mut d = with_clear_playfield();
    d.set_palette(17, 0xF00);
    let (pos, ctl) = encode_sprite_pos_ctl(30, 20, 22); // lines 20, 21
    d.write_sprite_pos(0, pos);
    d.write_sprite_ctl(0, ctl);
    d.write_sprite_datb(0, 0x0000);
    d.write_sprite_data(0, 0x8000);

    assert_eq!(
        d.output_pixel_color(30, 19),
        DeniseOcs::rgb12_to_argb32(0xF00),
        "manual data repeats before the DMA VSTART value"
    );
    assert_eq!(
        d.output_pixel_color(30, 20),
        DeniseOcs::rgb12_to_argb32(0xF00),
        "manual data displays on the encoded VSTART line"
    );
    assert_eq!(
        d.output_pixel_color(30, 22),
        DeniseOcs::rgb12_to_argb32(0xF00),
        "manual data repeats on the encoded VSTOP line"
    );

    d.write_sprite_ctl(0, ctl);
    assert_eq!(
        d.output_pixel_color(30, 23),
        DeniseOcs::rgb12_to_argb32(0x000),
        "SPRxCTL disarms the horizontal comparator"
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

// --- Direct-field arming, mid-line position writes, and CLXDAT
// fine-grained coverage (formerly inline tests). These complement the
// register-write tests above by poking fields directly to isolate
// individual pipeline stages. Helpers reuse `encode_sprite_pos_ctl`
// from this file. ---

#[test]
fn sprite_pixel_overrides_bitplane_pixel() {
    let mut denise = DeniseOcs::new();
    denise.set_palette(0, 0x000);
    denise.set_palette(1, 0x00F);
    denise.set_palette(17, 0xF00); // sprite 0/1 pair, color 1
    denise.bplcon2 = 0x0001; // PF1P=1 => sprite group 0 in front of PF1

    denise.bpl_shift[0] = 0x8000; // playfield color index 1
    denise.shift_count = 1;

    let (pos, ctl) = encode_sprite_pos_ctl(20, 10, 11);
    denise.spr_pos[0] = pos;
    denise.spr_ctl[0] = ctl;
    denise.spr_data[0] = 0x8000; // leftmost pixel = color code 1
    denise.spr_datb[0] = 0x0000;

    assert_eq!(
        denise.output_pixel_color(20, 10),
        DeniseOcs::rgb12_to_argb32(0xF00)
    );
}

#[test]
fn sprite_ctl_disarms_and_sprite_data_rearms_comparator() {
    let mut denise = DeniseOcs::new();
    denise.set_palette(0, 0x000);
    denise.set_palette(17, 0xF00);

    let (pos, ctl) = encode_sprite_pos_ctl(26, 10, 11);
    denise.write_sprite_pos(0, pos);
    denise.write_sprite_ctl(0, ctl); // disarm
    denise.write_sprite_datb(0, 0x0000);
    denise.write_sprite_data(0, 0x8000); // arm

    assert_eq!(
        denise.output_pixel_color(26, 10),
        DeniseOcs::rgb12_to_argb32(0xF00)
    );

    denise.write_sprite_ctl(0, ctl); // disarm again
    assert_eq!(
        denise.output_pixel_color(26, 10),
        DeniseOcs::rgb12_to_argb32(0x000),
        "writing SPRxCTL should disable sprite output until re-armed"
    );

    denise.write_sprite_datb(0, 0x0000); // DATB alone must not arm
    assert_eq!(
        denise.output_pixel_color(26, 10),
        DeniseOcs::rgb12_to_argb32(0x000),
        "writing SPRxDATB alone should not arm the comparator"
    );

    denise.write_sprite_data(0, 0x8000); // DATA arms
    assert_eq!(
        denise.output_pixel_color(26, 10),
        DeniseOcs::rgb12_to_argb32(0xF00),
        "writing SPRxDATA should arm the comparator"
    );
}

#[test]
fn sprite_pos_write_moves_armed_sprite_horizontally() {
    let mut denise = DeniseOcs::new();
    denise.set_palette(0, 0x000);
    denise.set_palette(17, 0x0F0);

    let (pos_a, ctl) = encode_sprite_pos_ctl(40, 12, 13);
    let (pos_b, _) = encode_sprite_pos_ctl(42, 12, 13);
    denise.write_sprite_pos(0, pos_a);
    denise.write_sprite_ctl(0, ctl);
    denise.write_sprite_datb(0, 0x0000);
    denise.write_sprite_data(0, 0x8000); // arm

    denise.write_sprite_pos(0, pos_b); // move while armed

    let c40 = denise.output_pixel_color(40, 12);
    let c42 = denise.output_pixel_color(42, 12);

    assert_eq!(
        c40,
        DeniseOcs::rgb12_to_argb32(0x000),
        "sprite should no longer appear at the old horizontal position"
    );
    assert_eq!(
        c42,
        DeniseOcs::rgb12_to_argb32(0x0F0),
        "writing SPRxPOS should move an armed sprite horizontally"
    );
}

#[test]
fn mid_line_sprite_data_write_affects_next_line_not_current_line() {
    let mut denise = DeniseOcs::new();
    denise.set_palette(0, 0x000);
    denise.set_palette(17, 0xF00);

    let (pos, ctl) = encode_sprite_pos_ctl(20, 10, 12); // active on lines 10 and 11
    denise.write_sprite_pos(0, pos);
    denise.write_sprite_ctl(0, ctl);
    denise.write_sprite_datb(0, 0x0000);
    denise.write_sprite_data(0, 0xC000); // first two pixels set

    // First pixel of line 10 loads and begins shifting.
    assert_eq!(
        denise.output_pixel_color(20, 10),
        DeniseOcs::rgb12_to_argb32(0xF00)
    );

    // Mid-line data rewrite should not affect the already-loaded serial data
    // for this line, but should be visible on the next line.
    denise.write_sprite_data(0, 0x0000);
    assert_eq!(
        denise.output_pixel_color(21, 10),
        DeniseOcs::rgb12_to_argb32(0xF00),
        "mid-line SPRxDATA write must not alter the current line after load"
    );

    assert_eq!(
        denise.output_pixel_color(20, 11),
        DeniseOcs::rgb12_to_argb32(0x000),
        "next line should use the newly written sprite data"
    );
}

#[test]
fn mid_line_sprite_pos_write_before_hstart_moves_same_line_trigger() {
    let mut denise = DeniseOcs::new();
    denise.set_palette(0, 0x000);
    denise.set_palette(17, 0x0FF);

    let (pos_a, ctl) = encode_sprite_pos_ctl(26, 9, 10);
    let (pos_b, _) = encode_sprite_pos_ctl(24, 9, 10);
    denise.write_sprite_pos(0, pos_a);
    denise.write_sprite_ctl(0, ctl);
    denise.write_sprite_datb(0, 0x0000);
    denise.write_sprite_data(0, 0x8000);

    denise.output_pixel(23, 9); // before either HSTART
    denise.write_sprite_pos(0, pos_b); // move before comparator hit
    let c24 = denise.output_pixel_color(24, 9);
    let c26 = denise.output_pixel_color(26, 9);

    assert_eq!(
        c24,
        DeniseOcs::rgb12_to_argb32(0x0FF),
        "SPRxPOS write before HSTART should affect the current line comparator hit"
    );
    assert_eq!(
        c26,
        DeniseOcs::rgb12_to_argb32(0x000),
        "sprite should not also trigger again at the old HSTART"
    );
}

#[test]
fn spritedata_rearm_after_hstart_waits_until_next_line() {
    let mut denise = DeniseOcs::new();
    denise.set_palette(0, 0x000);
    denise.set_palette(17, 0xF0F);

    let (pos, ctl) = encode_sprite_pos_ctl(28, 11, 13); // active on lines 11 and 12
    denise.write_sprite_pos(0, pos);
    denise.write_sprite_ctl(0, ctl); // disarm
    denise.write_sprite_datb(0, 0x0000);

    denise.output_pixel(29, 11); // HSTART already passed on line 11
    denise.write_sprite_data(0, 0x8000); // arm after HSTART
    assert_eq!(
        denise.output_pixel_color(30, 11),
        DeniseOcs::rgb12_to_argb32(0x000),
        "arming after HSTART should wait for the next line's comparison"
    );

    assert_eq!(
        denise.output_pixel_color(28, 12),
        DeniseOcs::rgb12_to_argb32(0xF0F),
        "next line should trigger output after late-line SPRxDATA arm"
    );
}

#[test]
fn clxdat_follows_loaded_sprite_serial_data_under_mid_line_data_write() {
    let mut denise = DeniseOcs::new();
    let (pos, ctl) = encode_sprite_pos_ctl(20, 10, 12); // active on lines 10 and 11
    denise.write_sprite_pos(0, pos);
    denise.write_sprite_ctl(0, ctl);
    denise.write_sprite_datb(0, 0x0000);
    denise.write_sprite_data(0, 0xC000); // two sprite pixels on each active line

    // First pixel on line 10 collides with odd bitplane.
    denise.bpl_shift[0] = 0x8000;
    denise.shift_count = 1;
    denise.output_pixel(20, 10);
    assert_eq!(denise.read_clxdat() & (1 << 1), 1 << 1);

    // Mid-line data rewrite should not affect the already-loaded serial data
    // for line 10, so the second pixel still collides.
    denise.write_sprite_data(0, 0x0000);
    denise.bpl_shift[0] = 0x8000;
    denise.shift_count = 1;
    denise.output_pixel(21, 10);
    assert_eq!(denise.read_clxdat() & (1 << 1), 1 << 1);

    // Next line uses the rewritten data, so no collision occurs.
    denise.bpl_shift[0] = 0x8000;
    denise.shift_count = 1;
    denise.output_pixel(20, 11);
    assert_eq!(denise.read_clxdat() & (1 << 1), 0);
}

#[test]
fn clxdat_stops_latching_after_mid_line_ctl_disarm() {
    let mut denise = DeniseOcs::new();
    let (pos, ctl) = encode_sprite_pos_ctl(24, 8, 9);
    denise.write_sprite_pos(0, pos);
    denise.write_sprite_ctl(0, ctl);
    denise.write_sprite_datb(0, 0x0000);
    denise.write_sprite_data(0, 0xC000); // two sprite pixels

    denise.bpl_shift[0] = 0x8000;
    denise.shift_count = 1;
    denise.output_pixel(24, 8);
    assert_eq!(denise.read_clxdat() & (1 << 1), 1 << 1);

    // Disarm mid-line before the second sprite pixel.
    denise.write_sprite_ctl(0, ctl);
    denise.bpl_shift[0] = 0x8000;
    denise.shift_count = 1;
    denise.output_pixel(25, 8);
    assert_eq!(
        denise.read_clxdat() & (1 << 1),
        0,
        "SPRxCTL disarm should stop further same-line sprite collisions"
    );
}

#[test]
fn clxdat_pos_write_before_hstart_moves_same_line_collision_point() {
    let mut denise = DeniseOcs::new();
    let (pos_a, ctl) = encode_sprite_pos_ctl(26, 9, 10);
    let (pos_b, _) = encode_sprite_pos_ctl(24, 9, 10);
    denise.write_sprite_pos(0, pos_a);
    denise.write_sprite_ctl(0, ctl);
    denise.write_sprite_datb(0, 0x0000);
    denise.write_sprite_data(0, 0x8000);

    denise.output_pixel(23, 9); // establish runtime before comparator hit
    let _ = denise.read_clxdat();

    denise.write_sprite_pos(0, pos_b); // move before HSTART

    denise.bpl_shift[0] = 0x8000;
    denise.shift_count = 1;
    denise.output_pixel(24, 9);
    assert_eq!(denise.read_clxdat() & (1 << 1), 1 << 1);

    denise.bpl_shift[0] = 0x8000;
    denise.shift_count = 1;
    denise.output_pixel(26, 9);
    assert_eq!(
        denise.read_clxdat() & (1 << 1),
        0,
        "collision should not also occur at the old HSTART after a pre-hit SPRxPOS move"
    );
}

#[test]
fn clxdat_arm_after_hstart_waits_until_next_line() {
    let mut denise = DeniseOcs::new();
    let (pos, ctl) = encode_sprite_pos_ctl(28, 11, 13); // active on lines 11 and 12
    denise.write_sprite_pos(0, pos);
    denise.write_sprite_ctl(0, ctl); // disarm
    denise.write_sprite_datb(0, 0x0000);

    denise.output_pixel(29, 11); // HSTART has passed on line 11
    denise.write_sprite_data(0, 0x8000); // arm late

    denise.bpl_shift[0] = 0x8000;
    denise.shift_count = 1;
    denise.output_pixel(30, 11);
    assert_eq!(
        denise.read_clxdat() & (1 << 1),
        0,
        "late-line arm must not cause a same-line collision after HSTART has passed"
    );

    denise.bpl_shift[0] = 0x8000;
    denise.shift_count = 1;
    denise.output_pixel(28, 12);
    assert_eq!(
        denise.read_clxdat() & (1 << 1),
        1 << 1,
        "next line should latch collision after late-line SPRxDATA arm"
    );
}

#[test]
fn transparent_sprite_pixel_leaves_playfield_visible_via_field_pokes() {
    let mut denise = DeniseOcs::new();
    denise.set_palette(0, 0x000);
    denise.set_palette(1, 0x0F0);
    denise.set_palette(17, 0xF00);

    denise.bpl_shift[0] = 0x8000; // playfield color index 1
    denise.shift_count = 1;

    let (pos, ctl) = encode_sprite_pos_ctl(24, 12, 13);
    denise.spr_pos[0] = pos;
    denise.spr_ctl[0] = ctl;
    denise.spr_data[0] = 0x0000;
    denise.spr_datb[0] = 0x0000; // transparent

    assert_eq!(
        denise.output_pixel_color(24, 12),
        DeniseOcs::rgb12_to_argb32(0x0F0)
    );
}

#[test]
fn lower_numbered_sprite_has_priority_on_overlap_via_field_pokes() {
    let mut denise = DeniseOcs::new();
    denise.set_palette(0, 0x000);
    denise.set_palette(17, 0xF00); // sprite 0 pair color 1
    denise.set_palette(21, 0x0FF); // sprite 2 pair color 1

    let (pos0, ctl0) = encode_sprite_pos_ctl(30, 8, 9);
    denise.spr_pos[0] = pos0;
    denise.spr_ctl[0] = ctl0;
    denise.spr_data[0] = 0x8000;
    denise.spr_datb[0] = 0x0000;

    let (pos2, ctl2) = encode_sprite_pos_ctl(30, 8, 9);
    denise.spr_pos[2] = pos2;
    denise.spr_ctl[2] = ctl2;
    denise.spr_data[2] = 0x8000;
    denise.spr_datb[2] = 0x0000;

    assert_eq!(
        denise.output_pixel_color(30, 8),
        DeniseOcs::rgb12_to_argb32(0xF00),
        "sprite 0 should appear in front of sprite 2"
    );
}

#[test]
fn attached_sprite_pair_uses_full_sprite_palette_range() {
    let mut denise = DeniseOcs::new();
    denise.set_palette(0, 0x000);
    denise.set_palette(25, 0x0F0); // attached color value 1001 => COLOR25

    let (pos, ctl) = encode_sprite_pos_ctl(32, 14, 15);
    denise.spr_pos[0] = pos;
    denise.spr_ctl[0] = ctl;
    denise.spr_data[0] = 0x8000; // even sprite code = 01
    denise.spr_datb[0] = 0x0000;

    denise.spr_pos[1] = pos;
    denise.spr_ctl[1] = ctl | 0x0080; // ATTACH on odd sprite
    denise.spr_data[1] = 0x0000;
    denise.spr_datb[1] = 0x8000; // odd sprite code = 10 (high two bits)

    assert_eq!(
        denise.output_pixel_color(32, 14),
        DeniseOcs::rgb12_to_argb32(0x0F0)
    );
}

#[test]
fn misaligned_attached_pair_reverts_to_shifted_color_subsets() {
    let mut denise = DeniseOcs::new();
    denise.set_palette(0, 0x000);
    denise.set_palette(17, 0xF00); // even-only attached fallback color (code 0001)
    denise.set_palette(20, 0x0F0); // odd-only attached fallback color (code 0100)

    // Sprites shift at lores rate (once per CCK = 2 beam_x steps).
    // To get a misaligned region where only ONE sprite has a pixel:
    // - Even sprite starts at hstart=38, with 1-bit data (0x8000).
    //   Its first pixel spans beam_x 38-39 (CCK 19). After shifting,
    //   the data is exhausted at beam_x 40+.
    // - Odd sprite starts at hstart=40, with 1-bit data (0x8000).
    //   Its first pixel spans beam_x 40-41 (CCK 20).
    //
    // At beam_x=38: even-only (even has data, odd hasn't started).
    // At beam_x=40: odd-only (even exhausted, odd just started).
    let (pos0, ctl0) = encode_sprite_pos_ctl(38, 10, 11);
    denise.spr_pos[0] = pos0;
    denise.spr_ctl[0] = ctl0;
    denise.spr_data[0] = 0x8000; // pixel at hstart=38 only
    denise.spr_datb[0] = 0x0000;

    let (pos1, ctl1) = encode_sprite_pos_ctl(40, 10, 11); // starts 1 CCK later
    denise.spr_pos[1] = pos1;
    denise.spr_ctl[1] = ctl1 | 0x0080; // ATTACH on odd sprite
    denise.spr_data[1] = 0x8000; // odd-only pixel at hstart=40
    denise.spr_datb[1] = 0x0000;

    let c38 = denise.output_pixel_color(38, 10);
    let c40 = denise.output_pixel_color(40, 10);

    assert_eq!(
        c38,
        DeniseOcs::rgb12_to_argb32(0xF00),
        "even-only pixel in misaligned attached pair should use COLOR17..19 subset"
    );
    assert_eq!(
        c40,
        DeniseOcs::rgb12_to_argb32(0x0F0),
        "odd-only pixel in misaligned attached pair should use shifted COLOR20/24/28 subset"
    );
}

#[test]
fn attach_bit_on_even_sprite_is_ignored() {
    let mut denise = DeniseOcs::new();
    denise.set_palette(0, 0x000);
    denise.set_palette(17, 0xF00); // would appear if sprite 2 were incorrectly treated as attached
    denise.set_palette(21, 0x00F); // normal sprite-2 color code 1 (group 1 base)

    let (pos, ctl) = encode_sprite_pos_ctl(44, 12, 13);
    denise.spr_pos[2] = pos;
    denise.spr_ctl[2] = ctl | 0x0080; // ATTACH bit on even sprite must be ignored
    denise.spr_data[2] = 0x8000;
    denise.spr_datb[2] = 0x0000;

    assert_eq!(
        denise.output_pixel_color(44, 12),
        DeniseOcs::rgb12_to_argb32(0x00F),
        "ATTACH is only valid on odd sprites; even sprite 2 should render as normal group-1 sprite"
    );
}

#[test]
fn bplcon2_pf1_priority_can_hide_sprite_group_0() {
    let mut denise = DeniseOcs::new();
    denise.set_palette(0, 0x000);
    denise.set_palette(1, 0x00F); // playfield color
    denise.set_palette(17, 0xF00); // sprite 0 color
    denise.bplcon2 = 0x0000; // PF1P = 0 => PF1 in front of all sprite groups

    denise.bpl_shift[0] = 0x8000; // playfield color index 1
    denise.shift_count = 1;

    let (pos, ctl) = encode_sprite_pos_ctl(18, 6, 7);
    denise.spr_pos[0] = pos;
    denise.spr_ctl[0] = ctl;
    denise.spr_data[0] = 0x8000;
    denise.spr_datb[0] = 0x0000;

    assert_eq!(
        denise.output_pixel_color(18, 6),
        DeniseOcs::rgb12_to_argb32(0x00F),
        "PF1 priority should place sprite 0 behind a nonzero playfield pixel"
    );
}

#[test]
fn bplcon2_pf1_priority_can_place_sprite_group_0_in_front() {
    let mut denise = DeniseOcs::new();
    denise.set_palette(0, 0x000);
    denise.set_palette(1, 0x00F); // playfield color
    denise.set_palette(17, 0xF00); // sprite 0 color
    denise.bplcon2 = 0x0001; // PF1P = 1 => SP01 in front of PF1

    denise.bpl_shift[0] = 0x8000; // playfield color index 1
    denise.shift_count = 1;

    let (pos, ctl) = encode_sprite_pos_ctl(19, 7, 8);
    denise.spr_pos[0] = pos;
    denise.spr_ctl[0] = ctl;
    denise.spr_data[0] = 0x8000;
    denise.spr_datb[0] = 0x0000;

    assert_eq!(
        denise.output_pixel_color(19, 7),
        DeniseOcs::rgb12_to_argb32(0xF00),
        "PF1 priority should allow sprite 0 in front when PF1P=1"
    );
}

#[test]
fn bplcon4_esprm_xors_even_sprite_colour_bank() {
    let mut denise = DeniseOcs::new();
    denise.set_palette(0, 0x000);
    // ESPRM = 1 => even sprites XOR upper nybble with 1.
    // Sprite 0 code 1: base index = 17 (0x11). XOR: 0x11 ^ 0x10 = 0x01.
    denise.bplcon4 = 0x0001;
    denise.set_palette(1, 0xABC);

    let (pos, ctl) = encode_sprite_pos_ctl(32, 14, 15);
    denise.spr_pos[0] = pos;
    denise.spr_ctl[0] = ctl;
    denise.spr_data[0] = 0x8000;
    denise.spr_datb[0] = 0x0000;

    assert_eq!(
        denise.output_pixel_color(32, 14),
        DeniseOcs::rgb12_to_argb32(0xABC),
        "ESPRM should XOR even sprite to palette index 1"
    );
}

#[test]
fn bplcon4_osprm_xors_odd_sprite_colour_bank() {
    let mut denise = DeniseOcs::new();
    denise.set_palette(0, 0x000);
    // OSPRM = 1 => odd sprites XOR upper nybble with 1.
    // Sprite 3 (odd, pair 1) code 1: base index = 20+1 = 21 (0x15).
    // XOR: 0x15 ^ 0x10 = 0x05 = 5.
    denise.bplcon4 = 0x0010;
    denise.set_palette(5, 0xDEF);

    let (pos, ctl) = encode_sprite_pos_ctl(32, 14, 15);
    denise.spr_pos[3] = pos;
    denise.spr_ctl[3] = ctl;
    denise.spr_data[3] = 0x8000;
    denise.spr_datb[3] = 0x0000;

    assert_eq!(
        denise.output_pixel_color(32, 14),
        DeniseOcs::rgb12_to_argb32(0xDEF),
        "OSPRM should XOR odd sprite to palette index 5"
    );
}

// --- Cov-5c additions: sprite-pair collision bits 10-14 + odd-sprite
// CLXCON enables for sprites 5 and 7 + the priority-loop "odd sprite
// with ATTACH continues" fall-through.

#[test]
fn clxdat_latches_sprite_pair_cross_bits_10_through_14() {
    // HRM Table 3-10: bits 10..14 cover the remaining sprite-pair-cross
    // combinations beyond bit 9 (SP01 × SP23). Drive groups in
    // combinations that hit each bit individually, with bitplanes
    // disabled so only the sprite-pair logic latches.
    fn arm_sprite_at(d: &mut DeniseOcs, sprite: usize, hstart: u16, vstart: u16, vstop: u16) {
        let (pos, ctl) = encode_sprite_pos_ctl(hstart, vstart, vstop);
        d.write_sprite_pos(sprite, pos);
        d.write_sprite_ctl(sprite, ctl);
        d.write_sprite_datb(sprite, 0x0000);
        d.write_sprite_data(sprite, 0x8000);
    }

    fn collide(s_a: usize, s_b: usize) -> u16 {
        let mut d = with_clear_playfield();
        d.clxcon = 0xFFFF; // enable all sprite-cross bits
        arm_sprite_at(&mut d, s_a, 30, 5, 6);
        arm_sprite_at(&mut d, s_b, 30, 5, 6);
        let _ = d.output_pixel_with_beam(30, 5, 30, 5);
        d.read_clxdat()
    }

    // bit 10 = group 0 × group 2  (sprites 0,1 × sprites 4,5) -> SP01 + SP45
    assert_ne!(collide(0, 4) & (1 << 10), 0, "SP01 × SP45 -> bit 10");
    // bit 11 = group 0 × group 3  (sprites 0,1 × sprites 6,7)
    assert_ne!(collide(0, 6) & (1 << 11), 0, "SP01 × SP67 -> bit 11");
    // bit 12 = group 1 × group 2  (sprites 2,3 × sprites 4,5)
    assert_ne!(collide(2, 4) & (1 << 12), 0, "SP23 × SP45 -> bit 12");
    // bit 13 = group 1 × group 3  (sprites 2,3 × sprites 6,7)
    assert_ne!(collide(2, 6) & (1 << 13), 0, "SP23 × SP67 -> bit 13");
    // bit 14 = group 2 × group 3  (sprites 4,5 × sprites 6,7)
    assert_ne!(collide(4, 6) & (1 << 14), 0, "SP45 × SP67 -> bit 14");
}

#[test]
fn clxcon_ensp5_and_ensp7_gate_odd_sprite_in_collision_mask() {
    // CLXCON bit 14 = ENSP5 (enables sprite 5 in collisions).
    // CLXCON bit 15 = ENSP7 (enables sprite 7 in collisions).
    // When the bit is clear, the odd sprite's pixels do not contribute
    // to its pair's group mask, so SP23 × SP45 (bit 12) and
    // SP23 × SP67 (bit 13) should NOT latch.
    fn arm(d: &mut DeniseOcs, sprite: usize, hstart: u16, vstart: u16, vstop: u16) {
        let (pos, ctl) = encode_sprite_pos_ctl(hstart, vstart, vstop);
        d.write_sprite_pos(sprite, pos);
        d.write_sprite_ctl(sprite, ctl);
        d.write_sprite_datb(sprite, 0x0000);
        d.write_sprite_data(sprite, 0x8000);
    }

    // Sprite 3 (group 1, even) plus sprite 5 (group 2, odd).
    // Bit 14 ENSP5 cleared while ENSP3 enabled -> sprite 5 dropped from
    // its group mask, group 2 stays at zero (no even sprite 4 active).
    let mut d = with_clear_playfield();
    d.clxcon = !(1u16 << 14); // clear ENSP5 only
    arm(&mut d, 3, 30, 5, 6);
    arm(&mut d, 5, 30, 5, 6);
    let _ = d.output_pixel_with_beam(30, 5, 30, 5);
    assert_eq!(
        d.read_clxdat() & (1 << 12),
        0,
        "ENSP5=0 should suppress SP23 × SP45 collision bit"
    );

    // Same shape with sprite 7 + ENSP7.
    let mut d = with_clear_playfield();
    d.clxcon = !(1u16 << 15);
    arm(&mut d, 3, 30, 5, 6);
    arm(&mut d, 7, 30, 5, 6);
    let _ = d.output_pixel_with_beam(30, 5, 30, 5);
    assert_eq!(
        d.read_clxdat() & (1 << 13),
        0,
        "ENSP7=0 should suppress SP23 × SP67 collision bit"
    );
}

#[test]
fn manual_sprite_uses_only_horizontal_comparator_above_line_511() {
    // An enhanced Agnus can DMA sprite data for VSTART=$301. Denise
    // must not independently decode the nine-bit OCS vertical fields
    // and reject that already-armed data.
    let mut d = with_clear_playfield();
    d.set_palette(17, 0xF00);
    d.write_sprite_pos(0, 0x0114); // VSTART low=$01, HSTART=40
    d.write_sprite_ctl(0, 0x0246); // enhanced VSTART=$301
    d.write_sprite_datb(0, 0x0000);
    d.write_sprite_data(0, 0x8000);

    assert_eq!(
        d.output_pixel_color(40, 0x0301),
        DeniseOcs::rgb12_to_argb32(0xF00),
        "armed data must reach the horizontal comparator on the enhanced line"
    );
}

#[test]
fn priority_loop_skips_attached_odd_sprite_then_lights_unattached_below() {
    // When an odd sprite has ATTACH set, the priority loop continues
    // (line 545-547 of chip.rs) so the next iteration can pick up a
    // lower-priority but standalone sprite. Build a scenario where
    // sprite 1 has ATTACH (skipped), sprite 0 is transparent (no pixel)
    // and sprite 2 lights — exercising the continue path on sprite 1
    // and falling through to sprite 2's standard arm.
    let mut d = with_clear_playfield();
    d.set_palette(17, 0xF00); // sprite 0/1 pair colour 1
    d.set_palette(21, 0x0F0); // sprite 2/3 pair colour 1

    // Sprite 0 (even) — armed but DATA=DATB=0 (transparent).
    let (pos0, ctl0) = encode_sprite_pos_ctl(30, 5, 6);
    d.write_sprite_pos(0, pos0);
    d.write_sprite_ctl(0, ctl0);
    d.write_sprite_datb(0, 0x0000);
    d.write_sprite_data(0, 0x0000);

    // Sprite 1 (odd) — ATTACH bit set, also transparent. The priority
    // loop must `continue` past this entry without matching its empty
    // payload.
    let (pos1, ctl1) = encode_sprite_pos_ctl(30, 5, 6);
    d.write_sprite_pos(1, pos1);
    d.write_sprite_ctl(1, ctl1 | 0x0080);
    d.write_sprite_datb(1, 0x0000);
    d.write_sprite_data(1, 0x0000);

    // Sprite 2 (even, standalone) — opaque.
    let (pos2, ctl2) = encode_sprite_pos_ctl(30, 5, 6);
    d.write_sprite_pos(2, pos2);
    d.write_sprite_ctl(2, ctl2);
    d.write_sprite_datb(2, 0x0000);
    d.write_sprite_data(2, 0x8000);

    assert_eq!(
        d.output_pixel_color(30, 5),
        DeniseOcs::rgb12_to_argb32(0x0F0),
        "attached transparent pair must fall through to next sprite"
    );
}

#[test]
fn exhausted_sprite_leaves_no_collision_trail_past_its_right_edge() {
    // Guards the concern raised in #459: once a sprite's serial shifter
    // exhausts, it must stop contributing its last colour code to the
    // collision latch, or a solid-to-the-edge sprite would phantom-collide
    // with any later sprite on the same line even though the two never
    // overlap a pixel. The per-pixel `spr_current_code = [0; 8]` reset in
    // `step_sprite_runtime_one_pixel` already makes this hold (so #459 was
    // not actually a live bug); this test locks that behaviour in.
    let mut d = with_clear_playfield();
    d.clxcon = 0xFFFF; // enable every collision source

    // Sprite 0 (group 0): armed at hstart=30, solid across its full
    // 16-pixel width so its rightmost emitted pixel is non-transparent
    // (the staleness trigger). It covers beam_x 30..=45, then exhausts.
    let (pos0, ctl0) = encode_sprite_pos_ctl(30, 5, 6);
    d.write_sprite_pos(0, pos0);
    d.write_sprite_ctl(0, ctl0);
    d.write_sprite_datb(0, 0x0000);
    d.write_sprite_data(0, 0xFFFF);

    // Sprite 2 (group 1): armed at hstart=60, well past sprite 0's right
    // edge — the two are never co-incident at any pixel.
    let (pos2, ctl2) = encode_sprite_pos_ctl(60, 5, 6);
    d.write_sprite_pos(2, pos2);
    d.write_sprite_ctl(2, ctl2);
    d.write_sprite_datb(2, 0x0000);
    d.write_sprite_data(2, 0xFFFF);

    // Resolve the pixel where sprite 2 is live but sprite 0 has long
    // since exhausted. Bit 9 is the SP01 ^ SP23 group cross.
    let _ = d.output_pixel_with_beam(60, 5, 60, 5);
    let clx = d.read_clxdat();
    assert_eq!(
        clx & (1 << 9),
        0,
        "exhausted sprite 0 must not phantom-collide with sprite 2 to its right"
    );
}

#[test]
fn wide_sprite_renders_pixels_beyond_the_16px_window() {
    // AGA wide sprites (#95): with `spr_width = 64` the shifter emits up
    // to 64 lores pixels per line, so a data bit at position 23 shows at
    // column hstart + (63 - 23) = +40 — a column a 16-px sprite can
    // never reach. The FMODE→spr_width wiring is what feeds this width;
    // here we pin the shifter capability it unlocks.
    let render_at_plus_40 = |width: u8| {
        let mut d = DeniseOcs::new();
        d.set_palette(0, 0x000); // COLOR00 = black background
        d.set_palette(17, 0xF00); // sprite 0/1 pair, colour code 1 = red
        d.spr_width = width;
        let (pos, ctl) = encode_sprite_pos_ctl(20, 10, 11);
        d.spr_pos[0] = pos;
        d.spr_ctl[0] = ctl;
        d.spr_data[0] = 1u64 << 23; // emits at hstart(20) + 40 = column 60
        d.spr_datb[0] = 0;
        d.output_pixel_color(60, 10)
    };

    assert_eq!(
        render_at_plus_40(64),
        DeniseOcs::rgb12_to_argb32(0xF00),
        "64-px sprite shows the bit-23 pixel 40 columns past hstart"
    );
    assert_eq!(
        render_at_plus_40(16),
        DeniseOcs::rgb12_to_argb32(0x000),
        "16-px sprite cannot reach column hstart+40 — background only"
    );
}

#[test]
fn wide_sprite_data_setter_loads_the_full_payload() {
    // #99: `write_sprite_data_wide` loads a 32/64-bit payload (the
    // DMA-assembled value) so the shifter can display columns past 16.
    // Here a 32-px sprite's second data word (bits 15-0) shows at
    // columns 16-31 — unreachable by a 16-px sprite.
    let make = |width: u8| {
        let mut d = DeniseOcs::new();
        d.set_palette(0, 0x000);
        d.set_palette(17, 0xF00); // sprite 0/1, colour code 1 = red
        d.spr_width = width;
        let (pos, ctl) = encode_sprite_pos_ctl(20, 10, 11);
        d.spr_pos[0] = pos;
        d.spr_ctl[0] = ctl;
        // Bit 8 set → emits at hstart + (31 - 8) = +23 (a column only a
        // 32-px sprite reaches; the low word holds it).
        d.write_sprite_data_wide(0, 1u64 << 8);
        d.write_sprite_datb_wide(0, 0);
        d.output_pixel_color(43, 10) // hstart(20) + 23
    };
    assert_eq!(
        make(32),
        DeniseOcs::rgb12_to_argb32(0xF00),
        "32-px sprite shows the bit-8 pixel 23 columns past hstart"
    );
    assert_eq!(
        make(16),
        DeniseOcs::rgb12_to_argb32(0x000),
        "16-px sprite cannot reach that column"
    );
}

#[test]
fn every_sprite_datb_write_lands_through_write_word() {
    // #468: the Denise sprite-register decode must reach SPR7DATB at
    // $17E. The range previously ended at $17C, so sprite 7's B-plane
    // write fell into the ignore arm and was silently dropped — sprite 7
    // rendered plane A only (the Flock unit 13 duck came out monochrome).
    // Every other sprite register fits below $17C, so only sprite 7 broke.
    let mut d = DeniseOcs::new();
    for sprite in 0..8u16 {
        let datb_reg = 0x140 + sprite * 8 + 6; // SPRxDATB
        let val = 0xA000 | sprite; // distinct, non-zero per sprite
        d.write_word(datb_reg, val);
        assert_eq!(
            d.spr_datb[sprite as usize],
            u64::from(val),
            "SPR{sprite}DATB (${datb_reg:03X}) write must land, not be dropped"
        );
    }
}
