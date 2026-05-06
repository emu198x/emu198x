//! Cov-5c — coverage of [`DeniseOcs::write_word`] dispatch and the
//! BPL1DAT-triggered queued shift-load path.
//!
//! Existing tests poke `bplcon0` etc. directly. Real machine traffic and
//! Copper MOVEs land in [`DeniseOcs::write_word`], which routes by Agnus
//! offset. This file exercises every arm of that dispatcher (BPLCON,
//! CLXCON, BPLnDAT, sprite, palette, ignored offsets) and the
//! `queue_shift_load_from_bpl1dat` → `apply_pending_shift_load_if_due`
//! pipeline that the dispatcher kicks off when BPL1DAT is written.

use commodore_denise_ocs::DeniseOcs;

#[test]
fn write_word_routes_bplcon0_bplcon1_bplcon2_bplcon4_clxcon() {
    let mut d = DeniseOcs::new();
    d.write_word(0x100, 0x9000); // BPLCON0
    d.write_word(0x102, 0x0040); // BPLCON1
    d.write_word(0x104, 0x0044); // BPLCON2
    d.write_word(0x10C, 0x00A5); // BPLCON4
    d.write_word(0x098, 0x1234); // CLXCON

    assert_eq!(d.bplcon0, 0x9000);
    assert_eq!(d.bplcon1, 0x0040);
    assert_eq!(d.bplcon2, 0x0044);
    assert_eq!(d.bplcon4, 0x00A5);
    assert_eq!(d.clxcon, 0x1234);
}

#[test]
fn write_word_palette_range_covers_all_indices() {
    let mut d = DeniseOcs::new();
    for idx in 0..32u16 {
        let val = idx * 0x0011;
        d.write_word(0x180 + idx * 2, val);
    }
    for idx in 0..32usize {
        assert_eq!(
            d.palette[idx],
            ((idx as u16) * 0x0011) & 0x0FFF,
            "palette[{idx}] not loaded by write_word"
        );
    }
}

#[test]
fn write_word_sprite_range_routes_to_pos_ctl_data_datb() {
    let mut d = DeniseOcs::new();

    // Sprite 3: $140 + 3*8 = $158 (POS), $15A (CTL), $15C (DATA), $15E (DATB).
    d.write_word(0x158, 0x0AB0);
    d.write_word(0x15A, 0x0040); // CTL bit 6 — disarms initially
    d.write_word(0x15C, 0xC0DE); // DATA arms the comparator
    d.write_word(0x15E, 0xBEEF);

    assert_eq!(d.spr_pos[3], 0x0AB0);
    assert_eq!(d.spr_ctl[3], 0x0040);
    assert_eq!(d.spr_data[3], 0xC0DE);
    assert_eq!(d.spr_datb[3], 0xBEEF);
    assert!(d.spr_armed[3], "DATA write should arm sprite");
}

#[test]
fn write_word_bpl1dat_queues_shift_load_then_pixel_pump_commits_it() {
    // BPL1DAT (offset 0x110) write must:
    //   1) populate `bpl_data[0]`
    //   2) queue a parallel shift-load
    //   3) the next `output_pixel_with_beam` then commits the load when
    //      the BPLCON1 phase comparator matches.
    let mut d = DeniseOcs::new();
    d.write_word(0x100, 0x1000); // BPLCON0 = LORES, BPU=1
    d.write_word(0x102, 0x0000); // BPLCON1 = no scroll → phase comparator at 0
    d.write_word(0x180, 0x000); // COLOR00
    d.write_word(0x182, 0xFFF); // COLOR01
    d.begin_beam_line();

    // Other-plane DAT writes don't queue the load.
    d.write_word(0x112, 0x0000); // BPL2DAT — won't trigger queue
    assert_eq!(d.bpl_data[1], 0x0000);

    // BPL1DAT triggers queue.
    d.write_word(0x110, 0x8000);
    assert_eq!(d.bpl_data[0], 0x8000);

    // Comparator phase logic uses (beam_x - 1) & mask. With odd_scroll=0,
    // even_scroll=0 and lores phase_mask=0x0F: phase = (1 - 1) & 0x0F = 0
    // matches both → both groups commit on this pixel.
    let dbg = d.output_pixel_with_beam(1, 0, 1, 0);
    assert_eq!(
        dbg.final_color_idx, 1,
        "BPL1DAT queued load should commit before pixel output"
    );
}

#[test]
fn bpl1dat_queue_with_nonzero_odd_scroll_delays_commit_by_phase_count() {
    // With odd_scroll=4 (BPLCON1=0x40 high nibble), the queued odd-plane
    // load only commits when (beam_x - 1) & 0x0F == 4 → beam_x = 5.
    // Earlier pixels see the previous (zero) shifter content.
    let mut d = DeniseOcs::new();
    d.write_word(0x100, 0x1000); // LORES, BPU=1
    d.write_word(0x102, 0x0040); // odd_scroll=4
    d.write_word(0x180, 0x000);
    d.write_word(0x182, 0xFFF);
    d.begin_beam_line();
    d.write_word(0x110, 0x8000); // queues BPL1DAT load

    // Pixels 0..4 see the still-empty shift register (nothing was loaded).
    for x in 0..5u32 {
        let dbg = d.output_pixel_with_beam(x, 0, x, 0);
        assert_eq!(
            dbg.final_color_idx, 0,
            "beam_x={x} should not yet see the queued load"
        );
    }

    // At beam_x=5, comparator phase = (5-1) & 0x0F = 4 = odd_scroll → commit.
    let dbg = d.output_pixel_with_beam(5, 0, 5, 0);
    assert_eq!(
        dbg.final_color_idx, 1,
        "queued load should commit when phase matches odd_scroll"
    );
}

#[test]
fn bpl1dat_queue_in_hires_uses_phase_mask_07() {
    // Hires BPLCON1 nibble drops the low bit (2-pixel granularity) and the
    // phase mask is 0x07 (8-cycle window). odd_scroll=2 -> commit at
    // (beam_x-1) & 0x07 == 2 -> beam_x = 3.
    let mut d = DeniseOcs::new();
    d.write_word(0x100, 0x9000); // HIRES + BPU=1
    d.write_word(0x102, 0x0030); // odd nibble=3 -> hires drops to 2
    d.write_word(0x180, 0x000);
    d.write_word(0x182, 0xFFF);
    d.begin_beam_line();
    d.write_word(0x110, 0x8000);

    // Pixels 0..2 see no commit yet.
    for x in 0..3u32 {
        let _ = d.output_pixel_with_beam(x, 0, x, 0);
    }
    // beam_x=3 -> phase=(3-1)&0x07=2 -> matches odd_scroll
    let dbg = d.output_pixel_with_beam(3, 0, 3, 0);
    // hires emits the loaded MSB twice within the call.
    assert_eq!(dbg.quad_samples[0].raw_color_idx, 1);
}

#[test]
fn bpl1dat_queue_split_phases_for_odd_and_even_planes() {
    // odd_scroll=1, even_scroll=3 -> the two plane groups commit at
    // different beam_x values. Verify each half lands at the right
    // phase: plane 0's MSB drives pixel index 1 at beam_x=2 (odd phase),
    // plane 1's MSB drives index 2 at beam_x=4 (even phase). Plane 0
    // has already drained one MSB by then so it stays low.
    let mut d = DeniseOcs::new();
    d.write_word(0x100, 0x2000); // LORES, BPU=2 (planes 0 + 1)
    d.write_word(0x102, 0x0013); // odd=1, even=3
    d.write_word(0x180, 0x000);
    d.write_word(0x182, 0xFFF); // index 1 (plane 0 only)
    d.write_word(0x184, 0xF00); // index 2 (plane 1 only)
    d.begin_beam_line();
    d.write_word(0x112, 0x8000); // BPL2DAT (plane 1) — no queue trigger
    d.write_word(0x110, 0x8000); // BPL1DAT (plane 0) — queues both groups

    // beam_x=2 -> phase=(2-1)&0x0F=1 = odd_scroll -> only odd commits.
    // Plane 0 emits MSB=1, plane 1 still empty -> color index 1.
    let dbg2 = d.output_pixel_with_beam(2, 0, 2, 0);
    assert_eq!(
        dbg2.final_color_idx, 1,
        "only odd group should have committed at this phase",
    );

    // beam_x=4 -> phase=3 = even_scroll -> even group commits.
    // Plane 0 has already shifted past its MSB so it contributes 0;
    // plane 1's freshly-loaded MSB drives index 2.
    let dbg4 = d.output_pixel_with_beam(4, 0, 4, 0);
    assert_eq!(
        dbg4.final_color_idx, 2,
        "even-scroll commit lights plane 1 alone at this beam"
    );
}

#[test]
fn write_word_unknown_offset_is_silently_ignored() {
    // Anything outside the documented Denise slice is a no-op.
    let mut d = DeniseOcs::new();
    let before = (
        d.bplcon0,
        d.bplcon1,
        d.bplcon2,
        d.bplcon4,
        d.clxcon,
        d.palette[0],
        d.bpl_data[0],
    );
    d.write_word(0x000, 0xFFFF); // out of range
    d.write_word(0x300, 0xFFFF); // out of range
    d.write_word(0x07E, 0xFFFF); // intregister, not Denise
    let after = (
        d.bplcon0,
        d.bplcon1,
        d.bplcon2,
        d.bplcon4,
        d.clxcon,
        d.palette[0],
        d.bpl_data[0],
    );
    assert_eq!(before, after, "unknown offsets must not mutate state");
}

#[test]
fn write_word_sprite_odd_offsets_within_range_are_ignored() {
    // The sprite range $140..$17C is iterated mod 8; offsets where
    // (offset & 7) ∈ {0, 2, 4, 6} hit POS/CTL/DATA/DATB. The match arm
    // for any other modulus is a no-op (defensive coverage of the
    // dispatcher's `_ => {}`).
    let mut d = DeniseOcs::new();
    d.write_word(0x141, 0xDEAD); // misaligned within sprite 0 row
    assert_eq!(d.spr_pos[0], 0);
    assert_eq!(d.spr_ctl[0], 0);
}
