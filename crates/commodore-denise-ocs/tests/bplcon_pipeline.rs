//! Phase 1 characterisation — BPLCON0/1/2 and the pixel pipeline.
//!
//! These tests lock in Denise archive behaviour as the spec against
//! which Phase 2 ports will land. They use the public crate API only
//! (no internal-field pokes beyond what the live machine will do),
//! so each test can be lifted into the live crate later with minimal
//! change.
//!
//! Covers task #151 from knowledge/amiga/denise-ocs-porting-gap-list.md:
//!   - LORES vs HIRES source-pixels-per-output-call
//!   - BPLCON1 odd/even barrel-shift scroll
//!   - BPLCON1 hires masks each delay to seven lores clocks
//!   - BPLCON1 prev-word carry across loads
//!   - BPLCON2 PF2PRI selects front playfield in DPF
//!   - DIW playfield-visible gate blanks to COLOR00
//!
//! PF1H uses bits 3:0, PF2H bits 7:4. Hires uses the low three
//! bits of each nibble, measured in two hires pixels per lores clock.
//!
//! Sprite+collision behaviour belongs to task #153.
//! HAM/EHB/DPF colour-resolve belongs to task #152.

use commodore_denise_ocs::DeniseOcs;

#[test]
fn lores_emits_one_source_pixel_per_output_call() {
    // HRM 3-3: "In low-resolution mode, the bit plane data is loaded
    // into the shifter and shifted out at the rate of 1 pixel per
    // color clock." Each output call consumes exactly one source
    // sample.
    let mut d = DeniseOcs::new();
    d.bplcon0 = 0x1000; // BPU=1, LORES
    d.set_palette(0, 0x000);
    d.set_palette(1, 0xF00);
    d.begin_beam_line();
    d.bpl_data[0] = 0xA000; // 1,0,1,0,...
    d.trigger_shift_load();

    let dbg = d.output_pixel_with_beam(0, 0, 0, 0);
    assert!(!dbg.hires);
    assert_eq!(dbg.source_pixels_per_fb_pixel, 1);
    assert_eq!(dbg.final_color_idx, 1);
    assert_eq!(d.shift_count, 15, "one LORES call consumes 1 pixel");
}

#[test]
fn hires_emits_two_source_pixels_per_output_call() {
    // HRM 3-3: HIRES doubles the serial shift rate; each output call
    // in our pipeline model therefore consumes 2 source pixels.
    let mut d = DeniseOcs::new();
    d.bplcon0 = 0x9000; // HIRES + BPU=1
    d.set_palette(0, 0x000);
    d.set_palette(1, 0xF00);
    d.begin_beam_line();
    d.bpl_data[0] = 0xC000; // 1,1,0,0,...
    d.trigger_shift_load();

    let dbg = d.output_pixel_with_beam(0, 0, 0, 0);
    assert!(dbg.hires);
    assert_eq!(dbg.source_pixels_per_fb_pixel, 2);
    assert_eq!(dbg.quad_samples[0].raw_color_idx, 1);
    assert_eq!(dbg.quad_samples[1].raw_color_idx, 1);
    assert_eq!(d.shift_count, 14, "one HIRES call consumes 2 pixels");
}

#[test]
fn bplcon1_lo_nibble_delays_plane_0() {
    // PF1H delays BPL1 (odd-numbered planes).
    let mut d = DeniseOcs::new();
    d.bplcon0 = 0x1000; // BPU=1 LORES
    d.bplcon1 = 0x0004; // odd scroll = 4 px for plane 0
    d.set_palette(0, 0x000);
    d.set_palette(1, 0xF00);
    d.begin_beam_line();
    d.bpl_data[0] = 0x8000;
    d.trigger_shift_load();

    for beam_x in 0..4 {
        assert_eq!(
            d.output_pixel_with_beam(0, 0, beam_x, 0).final_color_idx,
            0,
            "beam_x={beam_x} should still be delayed"
        );
    }
    assert_eq!(d.output_pixel_with_beam(0, 0, 4, 0).final_color_idx, 1);
}

#[test]
fn bplcon1_hi_nibble_delays_plane_1() {
    // PF2H delays BPL2 (even-numbered planes).
    let mut d = DeniseOcs::new();
    d.bplcon0 = 0x2000; // BPU=2 LORES
    d.bplcon1 = 0x0030; // even scroll = 3 px for plane 1
    d.set_palette(0, 0x000);
    d.set_palette(2, 0x0F0); // plane-1-only -> colour index 2
    d.begin_beam_line();
    d.bpl_data[0] = 0x0000;
    d.bpl_data[1] = 0x8000;
    d.trigger_shift_load();

    for beam_x in 0..3 {
        assert_eq!(
            d.output_pixel_with_beam(0, 0, beam_x, 0).final_color_idx,
            0,
            "beam_x={beam_x} should still be delayed"
        );
    }
    assert_eq!(d.output_pixel_with_beam(0, 0, 3, 0).final_color_idx, 2);
}

#[test]
fn hires_bplcon1_masks_high_bit_and_keeps_two_pixel_steps() {
    for delay in [1, 9] {
        let mut d = DeniseOcs::new();
        d.bplcon0 = 0x9000;
        d.bplcon1 = delay;
        d.begin_beam_line();
        d.bpl_data[0] = 0x8000;
        d.trigger_shift_load();
        let first = d.output_pixel_with_beam(0, 0, 0, 0);
        let second = d.output_pixel_with_beam(1, 0, 1, 0);
        assert_eq!(first.quad_samples[0].raw_color_idx, 0);
        assert_eq!(first.quad_samples[1].raw_color_idx, 0);
        assert_eq!(second.quad_samples[0].raw_color_idx, 1);
    }
}

#[test]
fn bplcon1_prev_word_carries_into_next_load() {
    // HRM Appendix C "Bitplane shifter": successive fetched words are
    // treated as a continuous stream; the scroll nibble pulls carry
    // bits from the previously loaded word.
    let mut d = DeniseOcs::new();
    d.bplcon0 = 0x1000;
    d.bplcon1 = 0x0001; // plane-0 scroll = 1 lores pixel
    d.set_palette(0, 0x000);
    d.set_palette(1, 0xFFF);
    d.begin_beam_line();

    d.bpl_data[0] = 0x0001; // first word: LSB set
    d.trigger_shift_load();
    // Drain 16 pixels of the first word (all zero except the bit we
    // expect to surface as carry).
    for beam_x in 0..16 {
        let _ = d.output_pixel_with_beam(0, 0, beam_x, 0);
    }

    d.bpl_data[0] = 0x0000; // second word: empty
    d.trigger_shift_load();
    // With scroll=1 the combined (prev << 16 | raw) >> 1 gives the
    // first word's LSB as the MSB of the loaded shift register.
    assert_eq!(
        d.output_pixel_with_beam(0, 0, 16, 0).final_color_idx,
        1,
        "prev-word carry should show first word's LSB at start of second load"
    );
}

#[test]
fn bplcon2_pf2pri_picks_front_playfield_in_dpf() {
    // HRM 3-18 dual-playfield: PF2PRI (BPLCON2 bit 6) selects which
    // playfield paints on top when both are non-zero.
    let mut d = DeniseOcs::new();
    d.bplcon0 = 0x2400; // BPU=2 + DBLPF
    d.set_palette(0, 0x000);
    d.set_palette(1, 0xF00); // PF1 colour 1
    d.set_palette(9, 0x00F); // PF2 colour 1 (8 + pf2_code=1)
    d.begin_beam_line();
    d.bpl_data[0] = 0x8000;
    d.bpl_data[1] = 0x8000;
    d.trigger_shift_load();

    // PF2PRI clear -> PF1 in front.
    d.bplcon2 = 0x0000;
    let dbg1 = d.output_pixel_with_beam(0, 0, 0, 0);
    assert_eq!(dbg1.final_color_idx, 1, "PF2PRI=0 picks PF1");

    // Reload and check PF2PRI set -> PF2 in front.
    d.begin_beam_line();
    d.bpl_data[0] = 0x8000;
    d.bpl_data[1] = 0x8000;
    d.trigger_shift_load();
    d.bplcon2 = 0x0040;
    let dbg2 = d.output_pixel_with_beam(0, 0, 0, 0);
    assert_eq!(dbg2.final_color_idx, 9, "PF2PRI=1 picks PF2");
}

fn encode_sprite_pos_ctl(hstart: u16, vstart: u16, vstop: u16) -> (u16, u16) {
    let pos = ((vstart & 0x00FF) << 8) | ((hstart >> 1) & 0x00FF);
    let ctl = ((vstop & 0x00FF) << 8)
        | (((vstart >> 8) & 1) << 2)
        | (((vstop >> 8) & 1) << 1)
        | (hstart & 1);
    (pos, ctl)
}

#[test]
fn dpf_pf2pri_sprite_priority_uses_pf2p_field_for_front_pf2() {
    // BPLCON2 bits 5:3 = PF2P (sprite group threshold for PF2).
    // In DPF with PF2PRI=1 (PF2 is front), sprite wins over PF2 only
    // when its group < PF2P. Verify both arms by flipping PF2P.
    let build = || {
        let mut d = DeniseOcs::new();
        d.bplcon0 = 0x2400; // BPU=2 + DBLPF
        d.set_palette(0, 0x000);
        d.set_palette(1, 0xFF0); // PF1 colour 1 (unused — PF1 is back)
        d.set_palette(9, 0x0F0); // PF2 colour 1 (front PF)
        d.set_palette(17, 0x00F); // sprite 0/1 pair colour 1
        d.begin_beam_line();
        d.bpl_data[0] = 0x8000; // PF1 lit
        d.bpl_data[1] = 0x8000; // PF2 lit
        d.queue_shift_load_from_bpl1dat();
        d.trigger_shift_load();
        let (pos, ctl) = encode_sprite_pos_ctl(0, 5, 6);
        d.write_sprite_pos(0, pos);
        d.write_sprite_ctl(0, ctl);
        d.write_sprite_datb(0, 0x0000);
        d.write_sprite_data(0, 0x8000);
        d
    };

    // PF2PRI=1 + PF2P=0 -> no sprite group < 0, PF2 wins.
    let mut d = build();
    d.bplcon2 = 0x0040; // PF2PRI, PF2P=0
    let dbg = d.output_pixel_with_beam(1, 5, 1, 5);
    assert_eq!(
        dbg.final_color_idx, 9,
        "PF2 in front + PF2P=0 -> PF2 wins over sprite group 0"
    );

    // PF2PRI=1 + PF2P=1 -> sprite group 0 < 1, sprite wins.
    let mut d = build();
    d.bplcon2 = 0x0048; // PF2PRI, PF2P=1
    let dbg = d.output_pixel_with_beam(1, 5, 1, 5);
    assert_eq!(
        dbg.final_color_idx, 17,
        "PF2 in front + PF2P=1 -> sprite group 0 wins"
    );
}

#[test]
fn legacy_bplcon0_zero_with_direct_shift_poke_drives_legacy_test_path() {
    // Cov-5c — covers the legacy fallback (chip.rs line 994-1007) where
    // BPLCON0 is still its default 0 but the test has seeded the shift
    // register directly. The compatibility shim infers `num_bpl` from
    // the seeded shift state instead of from BPLCON0's BPU field.
    let mut d = DeniseOcs::new();
    // Do NOT set BPLCON0. Direct-poke the shift state.
    d.set_palette(0, 0x000);
    d.set_palette(3, 0xFFF); // index 3 = planes 0+1 set
    d.bpl_shift[0] = 0x8000;
    d.bpl_shift[1] = 0x8000;
    d.shift_count = 1;

    let dbg = d.output_pixel_with_beam(0, 0, 0, 0);
    assert_eq!(
        dbg.final_color_idx, 3,
        "legacy direct-poke path should infer 2 planes from seeded shift state"
    );
}

#[test]
fn legacy_bplcon0_zero_with_only_bpl_shift_seeded_falls_back_to_bpl_shift() {
    // Same legacy path but the count array is empty — exercises the
    // `or_else` arm that scans `bpl_shift` itself for a non-zero plane.
    let mut d = DeniseOcs::new();
    d.set_palette(0, 0x000);
    d.set_palette(1, 0xF0F);
    d.bpl_shift[0] = 0x8000;
    // Explicitly leave bpl_shift_count at zero; set top-level shift_count.
    d.shift_count = 1;

    let dbg = d.output_pixel_with_beam(0, 0, 0, 0);
    assert_eq!(
        dbg.final_color_idx, 1,
        "legacy fallback should infer plane span from bpl_shift contents"
    );
}

#[test]
fn invisible_gate_suppresses_playfield_contribution() {
    // When DIW is closed for this pixel, Denise outputs COLOR00 and
    // does not latch bitplane bits into the collision register. The
    // raw `plane_bits_mask` in the debug output still reflects the
    // shifter contents — Denise's visibility gate affects composition
    // and collision latching, not the shifter itself.
    let mut d = DeniseOcs::new();
    d.bplcon0 = 0x1000;
    d.set_palette(0, 0x000);
    d.set_palette(1, 0xFFF);
    d.begin_beam_line();
    d.bpl_data[0] = 0xFFFF;
    d.trigger_shift_load();

    let dbg = d.output_pixel_with_beam_and_playfield_gate(0, 0, 0, 0, false);
    assert_eq!(
        dbg.final_color_idx, 0,
        "playfield_visible_gate=false outputs COLOR00"
    );
}
