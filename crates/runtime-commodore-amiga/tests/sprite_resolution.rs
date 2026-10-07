//! Restore Lisa inside a fractional-position, wide hires sprite stream.
use commodore_denise_aga::DeniseAga;
use common_commodore_amiga::DeniseChip;

#[test]
fn pending_hires_sprite_clock_and_wide_data_survive_restore() {
    for (fmode, width) in [(0, 16), (4, 32), (12, 64)] {
        let mut chip = DeniseAga::new();
        chip.set_bplcon0(0x1000);
        chip.write_word(0x106, 0x80);
        chip.write_word(0x1FC, fmode);
        chip.begin_beam_line();
        chip.enable_sprites_from_bpl1dat();
        chip.write_sprite_pos(0, 4);
        chip.write_sprite_ctl(0, 0x10);
        chip.as_inner_mut()
            .as_inner_mut()
            .write_sprite_datb_wide(0, 0);
        chip.as_inner_mut()
            .as_inner_mut()
            .write_sprite_data_wide(0, 0xA5A5_A5A5_A5A5_A5A5);
        for x in 0..9 {
            chip.output_pixel_with_beam_sprite_coords(x, 0, x, 0, x, 0, true);
        }
        let bytes = postcard::to_allocvec(&chip).expect("save in-flight hires sprite");
        let mut restored: DeniseAga = postcard::from_bytes(&bytes).expect("restore hires sprite");
        for x in 9..46 {
            let a = chip.output_pixel_with_beam_sprite_coords(x, 0, x, 0, x, 0, true);
            let b = restored.output_pixel_with_beam_sprite_coords(x, 0, x, 0, x, 0, true);
            assert_eq!(a, b);
            for sample in 0..4 {
                let position = x * 4 + sample;
                let active = (38..38 + width * 2).contains(&position);
                let bit = (position.saturating_sub(38) / 2) % 8;
                assert_eq!(
                    a.quad_is_sprite[sample as usize],
                    active && 0xA5 & (0x80 >> bit) != 0
                );
            }
            assert_eq!(chip.read_clxdat(), restored.read_clxdat());
            assert_eq!(
                chip.as_inner().as_inner().diagnostic_snapshot(),
                restored.as_inner().as_inner().diagnostic_snapshot()
            );
        }
    }
}

#[test]
fn pending_quarter_position_superhires_wide_sprite_survives_restore() {
    for (fmode, width) in [(0, 16), (4, 32), (12, 64)] {
        for (ctl, fractional) in [(8, 1), (0x10, 2), (0x18, 3)] {
            let mut chip = DeniseAga::new();
            chip.set_bplcon0(0x1000);
            chip.write_word(0x106, 0xC0);
            chip.write_word(0x1FC, fmode);
            chip.begin_beam_line();
            chip.enable_sprites_from_bpl1dat();
            chip.write_sprite_pos(0, 4);
            chip.write_sprite_ctl(0, ctl);
            chip.as_inner_mut()
                .as_inner_mut()
                .write_sprite_datb_wide(0, 0);
            chip.as_inner_mut()
                .as_inner_mut()
                .write_sprite_data_wide(0, 0xA5A5_A5A5_A5A5_A5A5);
            for x in 0..9 {
                chip.output_pixel_with_beam_sprite_coords(x, 0, x, 0, x, 0, true);
            }
            let bytes = postcard::to_allocvec(&chip).expect("save pending 35 ns sprite");
            let mut restored: DeniseAga =
                postcard::from_bytes(&bytes).expect("restore 35 ns sprite");
            for x in 9..28 {
                let a = chip.output_pixel_with_beam_sprite_coords(x, 0, x, 0, x, 0, true);
                let b = restored.output_pixel_with_beam_sprite_coords(x, 0, x, 0, x, 0, true);
                assert_eq!(a, b);
                for sample in 0..4 {
                    let position = x * 4 + sample;
                    let start = 36 + fractional;
                    let active = (start..start + width).contains(&position);
                    let bit = position.saturating_sub(start) % 8;
                    assert_eq!(
                        a.quad_is_sprite[sample as usize],
                        active && 0xA5 & (0x80 >> bit) != 0
                    );
                }
                assert_eq!(chip.read_clxdat(), restored.read_clxdat());
                assert_eq!(
                    chip.as_inner().as_inner().diagnostic_snapshot(),
                    restored.as_inner().as_inner().diagnostic_snapshot()
                );
            }
        }
    }
}
