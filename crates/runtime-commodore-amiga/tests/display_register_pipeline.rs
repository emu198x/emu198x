//! Pending Alice/Lisa register stages must survive a save-state boundary.
use commodore_agnus_ocs::Agnus;
use commodore_denise_aga::DeniseAga;
use commodore_denise_ocs::DeniseOcs;
use common_commodore_amiga::DeniseChip;

#[test]
fn restore_retains_shifted_bits_above_the_narrow_output_tap() {
    let mut chip = DeniseAga::new();
    chip.set_bplcon0(0x1040);
    chip.begin_beam_line();
    chip.load_bitplane(0, 0xA5A5);
    chip.as_inner_mut().as_inner_mut().trigger_shift_load();
    for x in 0..4 {
        chip.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true);
    }
    assert_eq!(
        chip.as_inner()
            .as_inner()
            .diagnostic_snapshot()
            .bitplanes
            .shift_data_32[0],
        0xA5A5_0000,
    );
    let mut restored: DeniseAga =
        postcard::from_bytes(&postcard::to_allocvec(&chip).expect("encode retained upper half"))
            .expect("restore retained upper half");
    // Isolate the output-tap change from timed FMODE/BPL1DAT propagation.
    chip.as_inner_mut().as_inner_mut().bitplane_fmode = 1;
    restored.as_inner_mut().as_inner_mut().bitplane_fmode = 1;
    let mut samples = Vec::new();
    for x in 4..9 {
        let expected = chip.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true);
        let actual = restored.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true);
        assert_eq!(
            actual.quad_playfield_color_idx,
            expected.quad_playfield_color_idx
        );
        samples.extend(actual.quad_playfield_color_idx);
    }
    let expected: Vec<_> = (0..16)
        .map(|bit| u8::from(0xA5A5 & (0x8000 >> bit) != 0))
        .collect();
    assert_eq!(&samples[..16], expected);
}

#[test]
fn restore_preserves_each_pending_dma_register_copy() {
    let mut chip = Agnus::new();
    chip.max_bitplanes = 8;
    chip.agnus_id = 0x2300;
    chip.bplcon0 = 0x1000;
    chip.write_bplcon0(0x9000);
    chip.tick_cck();
    // A second bus write must retain the earlier write leaving the stage.
    chip.write_bplcon0(0x2040);
    let mut restored: Agnus = postcard::from_bytes(&postcard::to_allocvec(&chip).expect("encode"))
        .expect("restore pending DMA copies");
    for expected in [0x1000, 0x1000, 0x9000, 0x2040] {
        chip.tick_cck();
        restored.tick_cck();
        assert_eq!(restored.dma_bplcon0(), expected);
        assert_eq!(restored.dma_bplcon0(), chip.dma_bplcon0());
    }
}

#[test]
fn restore_preserves_resolution_and_xor_writes_between_native_samples() {
    let mut chip = DeniseAga::new();
    chip.set_bplcon0(0x1000);
    chip.palette_24[0] = 0x000011;
    chip.palette_24[1] = 0x00FF00;
    chip.output_pixel_with_beam_and_playfield_gate(0, 0, 0, 0, true);
    chip.write_word(0x100, 0x1040);
    chip.write_word(0x1FC, 3);
    chip.advance_register_output_pipeline();
    chip.write_word(0x10C, 0x0111);
    for sample in 0..5 {
        chip.resolve_output_sample_argb(1, 1, false, sample % 4);
    }
    let mut restored: DeniseAga =
        postcard::from_bytes(&postcard::to_allocvec(&chip).expect("encode"))
            .expect("restore pending output copies");
    for offset in 5..16 {
        let expected = if offset < 6 { 0xFF00_0011 } else { 0xFF00_FF00 };
        assert_eq!(
            restored.resolve_output_sample_argb(1, 1, false, offset % 4),
            expected
        );
        assert_eq!(
            chip.resolve_output_sample_argb(1, 1, false, offset % 4),
            expected
        );
        if offset % 4 == 3 {
            chip.advance_register_output_pipeline();
            restored.advance_register_output_pipeline();
        }
        assert_eq!(restored.diagnostic_snapshot(), chip.diagnostic_snapshot());
        assert_eq!(restored.bplcon0(), chip.bplcon0());
    }
    assert_eq!(restored.bplcon0(), 0x1040);
    restored.queue_shift_load_from_bpl1dat();
    assert_eq!(restored.as_inner().as_inner().bitplane_fmode, 3);
}

#[test]
fn restore_rejects_a_serial_history_cursor_outside_its_native_ring() {
    let mut value = serde_json::to_value(DeniseOcs::new()).expect("encode chip");
    value["bitplane_output_cursor"] = serde_json::json!(512);
    let error = serde_json::from_value::<DeniseOcs>(value)
        .err()
        .expect("reject invalid cursor");
    assert!(
        error
            .to_string()
            .contains("invalid Lisa serial scroll cursor")
    );
}

#[test]
fn restore_preserves_a_partially_completed_serial_period_after_a_rate_change() {
    let mut chip = DeniseAga::new();
    chip.set_bplcon0(0x1040);
    chip.begin_beam_line();
    chip.load_bitplane(0, 1);
    chip.as_inner_mut().as_inner_mut().trigger_shift_load();
    for x in 0..4 {
        chip.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true);
    }
    chip.set_bplcon0(0x9000);
    let transition = chip.output_pixel_with_beam_and_playfield_gate(4, 0, 4, 0, true);
    assert_eq!(transition.quad_playfield_color_idx, [1, 0, 0, 0]);
    let mut restored: DeniseAga =
        postcard::from_bytes(&postcard::to_allocvec(&chip).expect("encode"))
            .expect("restore partial serial period");
    for x in 5..12 {
        let expected = chip.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true);
        let actual = restored.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true);
        assert_eq!(
            actual.quad_playfield_color_idx,
            expected.quad_playfield_color_idx
        );
        if x == 5 {
            assert_eq!(actual.quad_playfield_color_idx, [0; 4]);
        }
        assert_eq!(
            restored
                .as_inner()
                .as_inner()
                .diagnostic_snapshot()
                .bitplanes,
            chip.as_inner().as_inner().diagnostic_snapshot().bitplanes
        );
    }
}

#[test]
fn restore_preserves_old_cadence_slots_with_the_new_live_transfer_width() {
    let mut chip = Agnus::new();
    chip.agnus_id = 0x2300;
    chip.max_bitplanes = 8;
    chip.bplcon0 = 0x1040;
    chip.dmacon = 0x0300;
    chip.ddfstrt = 0x38;
    chip.ddfstop = 0xD0;
    chip.diwstrt = 0x2C81;
    chip.diwstop = 0xF4C1;
    chip.vpos = 132;
    chip.fmode = 1;
    while chip.hpos < 130 {
        chip.tick_cck();
    }
    chip.write_fmode(3);
    let mut restored: Agnus = postcard::from_bytes(&postcard::to_allocvec(&chip).expect("encode"))
        .expect("restore pending width cadence");
    for expected in [Some(0), None, None, None, Some(0)] {
        chip.tick_cck();
        restored.tick_cck();
        assert_eq!(restored.cck_bus_plan().bitplane_dma_fetch_plane, expected);
        assert_eq!(restored.cck_bus_plan(), chip.cck_bus_plan());
        assert_eq!(restored.bpl_fetch_width(), 4);
    }
}

#[test]
fn restore_rejects_a_serial_phase_outside_its_four_native_periods() {
    let mut value = serde_json::to_value(DeniseOcs::new()).expect("encode chip");
    value["bitplane_serial_phase"] = serde_json::json!(4);
    let error = serde_json::from_value::<DeniseOcs>(value)
        .err()
        .expect("reject invalid phase");
    assert!(
        error
            .to_string()
            .contains("invalid Lisa serial clock phase")
    );
}

#[test]
fn restore_preserves_independent_playfield_clocks_and_pending_copies() {
    let mut chip = DeniseAga::new();
    chip.set_bplcon0(0x2440);
    chip.write_word(0x1FC, 3);
    chip.begin_beam_line();
    chip.write_word(0x102, 0x0035);
    for _ in 0..3 {
        chip.advance_register_output_pipeline();
    }
    chip.load_bitplane(0, 0xA55A);
    chip.load_bitplane(1, 0x6C39);
    chip.queue_shift_load_from_bpl1dat();
    for x in 0..5 {
        chip.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true);
    }
    // Change source rate after the even copy and before the odd copy.
    // The odd copy restarts its new-rate clock; the even stream retains
    // the partially completed old-rate period.
    chip.set_bplcon0(0xA400);
    for x in 5..7 {
        chip.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true);
    }
    let before = chip.as_inner().as_inner().diagnostic_snapshot().bitplanes;
    assert_ne!(before.serial_clock_phase, before.serial_clock_phase_even);
    chip.load_bitplane(0, 0x37B1);
    chip.load_bitplane(1, 0x1F3D);
    chip.queue_shift_load_from_bpl1dat();
    chip.write_word(0x102, 0x02CB);
    let mut restored: DeniseAga =
        postcard::from_bytes(&postcard::to_allocvec(&chip).expect("encode independent playfields"))
            .expect("restore independent playfields");
    assert_eq!(restored.diagnostic_snapshot(), chip.diagnostic_snapshot());
    for x in 7..40 {
        let expected = chip.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true);
        let actual = restored.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true);
        assert_eq!(actual, expected, "output after restore at {x}");
        assert_eq!(restored.diagnostic_snapshot(), chip.diagnostic_snapshot());
    }
}

#[test]
fn restore_rejects_an_even_playfield_phase_outside_its_native_period() {
    let mut value = serde_json::to_value(DeniseOcs::new()).expect("encode chip");
    value["bitplane_serial_phase_even"] = serde_json::json!(4);
    let error = serde_json::from_value::<DeniseOcs>(value)
        .err()
        .expect("reject invalid even-group phase");
    assert!(
        error
            .to_string()
            .contains("invalid Lisa serial clock phase")
    );
}

#[test]
fn restore_rejects_a_pending_tail_larger_than_its_holding_register() {
    let mut value = serde_json::to_value(DeniseOcs::new()).expect("encode chip");
    value["bpl_pending_tail_len"] = serde_json::json!([4, 0, 0, 0, 0, 0, 0, 0]);
    let error = serde_json::from_value::<DeniseOcs>(value)
        .err()
        .expect("reject invalid pending tail");
    assert!(
        error
            .to_string()
            .contains("invalid Lisa pending fetch-tail length")
    );
}
