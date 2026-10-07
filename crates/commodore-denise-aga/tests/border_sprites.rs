//! Lisa BRDRSPRT: ECSENA-gated BPLCON3 bit 1 bypasses DIW and BPL1DAT clipping.
use commodore_denise_aga::DeniseAga;
use common_commodore_amiga::DeniseChip;

fn fixture(ecsena: bool, control: u16, bpl1dat: bool, data: u16) -> DeniseAga {
    let mut denise = DeniseAga::new();
    denise.set_bplcon0(u16::from(ecsena));
    denise.write_word(0x106, control);
    denise.write_word(0x10C, 0xF0B1); // XOR must not colour the border or sprite
    denise.begin_beam_line();
    denise.write_sprite_pos(0, 0);
    denise.write_sprite_ctl(0, 0);
    denise.write_sprite_datb(0, 0);
    denise.write_sprite_data(0, data);
    if bpl1dat {
        denise
            .as_inner_mut()
            .as_inner_mut()
            .enable_sprites_from_bpl1dat();
    }
    denise
}

#[test]
fn border_sprite_requires_bit_one_and_ecsena_but_bypasses_bpl1dat() {
    for ecsena in [false, true] {
        for control in [0, 2, 4, 6] {
            for bpl1dat in [false, true] {
                let mut denise = fixture(ecsena, control, bpl1dat, 0xFFFF);
                for x in 0..4 {
                    let sample =
                        denise.output_pixel_with_beam_sprite_coords(x, 0, x, 0, x, 0, false);
                    let visible = x >= 1 && ecsena && control & 2 != 0;
                    assert_eq!(
                        sample.quad_is_sprite[0], visible,
                        "ECSENA={ecsena}, BPLCON3={control:04x}, BPL1DAT={bpl1dat}, x={x}"
                    );
                    assert_eq!(sample.final_color_idx, if visible { 0xB1 } else { 0 });
                    assert_eq!(sample.quad_playfield_color_idx, [0; 4]);
                }
            }
        }
    }
}

#[test]
fn toggling_border_visibility_does_not_restart_the_sprite_serial_stream() {
    let mut denise = fixture(true, 0, true, 0xF0F0);
    for x in 0..18 {
        if x == 8 {
            denise.write_word(0x106, 2);
        }
        if x == 12 {
            denise.set_bplcon0(0);
        }
        let sample = denise.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, false);
        assert_eq!(sample.quad_is_sprite[0], (9..12).contains(&x), "x={x}");
    }
}

#[test]
fn ordinary_display_sprites_do_not_require_the_border_switch() {
    let mut denise = fixture(false, 0, true, 0xFFFF);
    for x in 0..4 {
        let sample = denise.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true);
        assert_eq!(sample.quad_is_sprite[0], x >= 1);
    }
}

#[test]
fn border_sprite_resolves_its_banked_palette_and_keeps_border_color_zero() {
    let mut denise = fixture(true, 2, false, 0xFFFF);
    denise.write_word(0x106, 0xA000); // palette bank 5
    denise.write_word(0x1A2, 0x0F00); // COLOR177: sprite colour $B1
    denise.advance_color_output_samples(1);
    denise.write_word(0x106, 2);
    for x in 0..4 {
        let sample = denise.output_pixel_with_beam_sprite_coords(x, 0, x, 0, x, 0, false);
        let rgb = denise.resolve_output_color_argb(
            sample.quad_playfield_color_idx[0],
            sample.quad_color_idx[0],
            sample.quad_is_sprite[0],
        );
        assert_eq!(rgb, if x >= 1 { 0xFFFF_0000 } else { 0xFF00_0000 });
    }
}

#[test]
fn border_sprites_collide_against_other_sprites_and_zero_playfield_data() {
    let mut denise = fixture(true, 2, false, 0xFFFF);
    denise.write_word(0x098, 0x0041); // require BP1=1: blanked border data cannot match
    denise.write_sprite_pos(2, 0);
    denise.write_sprite_ctl(2, 0);
    denise.write_sprite_datb(2, 0);
    denise.write_sprite_data(2, 0xFFFF);
    for x in 0..3 {
        denise.output_pixel_with_beam_sprite_coords(x, 0, x, 0, x, 0, false);
        let collision = denise.read_clxdat();
        assert_eq!(collision & (1 << 9), if x >= 1 { 1 << 9 } else { 0 });
        assert_eq!(collision & ((1 << 1) | (1 << 2)), 0);
    }
}

#[test]
fn ocs_and_ecs_do_not_gain_lisas_border_sprite_switch() {
    fn emit<D: DeniseChip>(mut denise: D) {
        denise.write_word(0x106, 2);
        for x in 0..4 {
            let sample = denise.output_pixel_with_beam_sprite_coords(x, 0, x, 0, x, 0, false);
            assert_eq!(sample.quad_is_sprite, [false; 4]);
            assert_eq!(sample.final_color_idx, 0);
        }
    }
    let mut ocs = commodore_denise_ocs::DeniseOcs::new();
    ocs.bplcon0 = 1;
    ocs.begin_beam_line();
    ocs.write_sprite_pos(0, 0);
    ocs.write_sprite_ctl(0, 0);
    ocs.write_sprite_datb(0, 0);
    ocs.write_sprite_data(0, 0xFFFF);
    ocs.enable_sprites_from_bpl1dat();
    emit(commodore_denise_ecs::DeniseEcs::from_ocs(ocs.clone()));
    emit(ocs);
}
