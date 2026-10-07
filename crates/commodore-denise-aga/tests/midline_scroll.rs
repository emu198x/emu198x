//! Physical copy timing must remain independent for the two playfields.
//! Reference: FS-UAE drawing.cpp::lts_unaligned_aga and update_bplcon1.
use commodore_denise_aga::DeniseAga;
use common_commodore_amiga::DeniseChip;

#[test]
fn changing_scroll_retargets_pending_copy_without_loading_the_other_group() {
    let mut chip = DeniseAga::new();
    chip.set_bplcon0(0x2400);
    chip.begin_beam_line();
    chip.write_word(0x102, 0x0035);
    for _ in 0..3 {
        chip.advance_register_output_pipeline();
    }
    chip.load_bitplane(0, 0xA55A);
    chip.load_bitplane(1, 0x6C39);
    chip.queue_shift_load_from_bpl1dat();
    for x in 0..4 {
        chip.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true);
    }
    let pending = chip.as_inner().as_inner().diagnostic_snapshot().bitplanes;
    assert!(pending.pending_copy_odd_planes);
    assert!(pending.pending_copy_even_planes);
    chip.output_pixel_with_beam_and_playfield_gate(4, 0, 4, 0, true);
    let copied = chip.as_inner().as_inner().diagnostic_snapshot().bitplanes;
    assert!(copied.pending_copy_odd_planes);
    assert!(!copied.pending_copy_even_planes);
    // The odd copy was due at physical phase 5. Retarget it to phase 11
    // before it happens; the even stream must continue its current word.
    chip.write_word(0x102, 0x003B);
    for _ in 0..3 {
        chip.advance_register_output_pipeline();
    }
    for x in 5..12 {
        chip.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true);
        let state = chip.as_inner().as_inner().diagnostic_snapshot().bitplanes;
        assert!(
            state.pending_copy_odd_planes,
            "odd word loaded early at {x}"
        );
        assert!(!state.pending_copy_even_planes);
    }
    chip.output_pixel_with_beam_and_playfield_gate(12, 0, 12, 0, true);
    let state = chip.as_inner().as_inner().diagnostic_snapshot().bitplanes;
    assert!(!state.pending_copy_odd_planes);
    assert!(!state.pending_copy_even_planes);
    assert_eq!(state.shift_counts[0], 15);
    assert!(state.shift_counts[1] < state.shift_counts[0]);
}

#[test]
fn pending_wide_copy_keeps_its_head_and_tail_from_the_same_bpl1dat_group() {
    let mut chip = DeniseAga::new();
    chip.set_bplcon0(0x2400);
    chip.write_word(0x1FC, 1);
    chip.write_word(0x102, 0x00F0);
    for _ in 0..3 {
        chip.advance_register_output_pipeline();
    }
    chip.begin_beam_line();
    chip.load_bitplane(0, 0xA55A);
    chip.push_bpl_fifo(0, 0x1F3D);
    chip.load_bitplane(1, 0x6C39);
    chip.push_bpl_fifo(1, 0x1234);
    chip.queue_shift_load_from_bpl1dat();
    // A subsequent BPL2 transfer reaches the raw holding register before
    // the previous group's delayed even copy. It must not alter that copy.
    chip.load_bitplane(1, 0x37B1);
    chip.push_bpl_fifo(1, 0x5678);
    for x in 0..17 {
        chip.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true);
    }
    let state = chip.as_inner().as_inner().diagnostic_snapshot().bitplanes;
    assert!(!state.pending_copy_even_planes);
    assert_eq!(state.holding_data[1], 0x37B1);
    assert_eq!(state.pending_data[1], 0x6C39);
    assert_eq!(state.pending_fetch_tails[1][0], 0x1234);
    assert_eq!(state.active_fifo[1][0], 0x1234);
    assert_eq!(state.staged_fetch_tails[1][0], 0x5678);
}

#[test]
fn scroll_mirror_updates_before_the_normal_stage_selector() {
    let mut chip = DeniseAga::new();
    chip.write_word(0x102, 0x02CB);
    assert_eq!(chip.as_inner().as_inner().bplcon1, 0x02CB);
    for _ in 0..2 {
        chip.advance_register_output_pipeline();
        assert_eq!(chip.diagnostic_snapshot().bplcon1_visible, 0);
    }
    chip.advance_register_output_pipeline();
    assert_eq!(chip.diagnostic_snapshot().bplcon1_visible, 0x02CB);
}
