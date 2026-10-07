//! Lisa normal RGA stage, separate from the raw register input.
use commodore_denise_aga::DeniseAga;
use common_commodore_amiga::DeniseChip;

#[test]
fn resolution_reaches_the_same_normal_stage_as_the_existing_selectors() {
    let mut chip = DeniseAga::new();
    chip.set_bplcon0(0x1000);
    chip.write_word(0x100, 0x9041);
    for tick in 0..3 {
        assert_eq!(chip.bplcon0(), 0x1000, "normal stage before tick {tick}");
        chip.advance_register_output_pipeline();
    }
    assert_eq!(chip.bplcon0(), 0x9041);
}

#[test]
fn changing_serial_rate_keeps_the_preceding_lores_bit_for_its_full_native_period() {
    let mut chip = DeniseAga::new();
    chip.set_bplcon0(0x1000);
    chip.begin_beam_line();
    chip.write_word(0x110, 0x8000);
    for x in 0..2 {
        let output = chip.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true);
        assert_eq!(output.quad_playfield_color_idx, [u8::from(x == 1); 4]);
    }
    // Change the active serial rate directly to isolate serial history from
    // RGA propagation. The previous lores sample occupied four 35 ns periods.
    chip.set_bplcon0(0x1040);
    let output = chip.output_pixel_with_beam_and_playfield_gate(2, 0, 2, 0, true);
    assert_eq!(output.quad_playfield_color_idx, [0; 4]);
    let output = chip.output_pixel_with_beam_and_playfield_gate(3, 0, 3, 0, true);
    assert_eq!(output.quad_playfield_color_idx, [0; 4]);
}

#[test]
fn reducing_the_serial_rate_preserves_the_partly_completed_shift_period() {
    for (old_mode, new_mode, word) in [(0x1040, 0x9000, 0x0001), (0x9000, 0x1000, 0x0100)] {
        let mut chip = DeniseAga::new();
        chip.set_bplcon0(old_mode);
        chip.begin_beam_line();
        chip.load_bitplane(0, word);
        chip.as_inner_mut().as_inner_mut().trigger_shift_load();
        for x in 0..4 {
            chip.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true);
        }
        chip.set_bplcon0(new_mode);
        let output = chip.output_pixel_with_beam_and_playfield_gate(4, 0, 4, 0, true);
        assert_eq!(
            output.quad_playfield_color_idx,
            [1, 0, 0, 0],
            "mode {old_mode:04x}→{new_mode:04x}"
        );
    }
}

#[test]
fn fetch_width_crosses_normal_rga_then_latches_on_bpl1dat() {
    let mut chip = DeniseAga::new();
    chip.write_word(0x1FC, 3);
    assert_eq!(chip.as_inner().as_inner().bitplane_fmode, 0);
    for _ in 0..3 {
        chip.advance_register_output_pipeline();
    }
    assert_eq!(
        chip.as_inner().as_inner().bitplane_fmode,
        0,
        "old data retains its copy width"
    );
    chip.write_word(0x110, 0xA5A5);
    assert_eq!(chip.as_inner().as_inner().bitplane_fmode, 3);
}

#[test]
fn wide_hires_copy_uses_the_physical_counter_independently_of_ddf_origin() {
    let mut absolute = DeniseAga::new();
    absolute.set_bplcon0(0x9000);
    absolute.write_word(0x1FC, 3);
    for _ in 0..3 {
        absolute.advance_register_output_pipeline();
    }
    absolute.begin_beam_line();
    absolute.write_word(0x110, 0xA5A5);
    let mut relative = absolute.clone();
    let mut visible = false;
    for beam in 112..160 {
        let expected =
            absolute.output_pixel_with_beam_sprite_coords(beam, 0, beam, 0, beam, 0, true);
        let actual =
            relative.output_pixel_with_beam_sprite_coords(beam, 0, beam - 112, 0, beam, 0, true);
        assert_eq!(
            actual.quad_playfield_color_idx,
            expected.quad_playfield_color_idx
        );
        visible |= expected.quad_playfield_color_idx.contains(&1);
    }
    assert!(
        visible,
        "the pending data must actually cross the copy boundary"
    );
}
