//! Sprite clocks are independent of playfield clocks; phase constrained by
//! the neutral A1200 DMA probe, not by resizing a lores image.
use commodore_denise_aga::DeniseAga;
use common_commodore_amiga::DeniseChip;

fn stream(playfield: u16, spres: u16, ctl: u16, data: u16) -> Vec<u8> {
    let mut chip = DeniseAga::new();
    chip.set_bplcon0(0x1000 | playfield);
    chip.write_word(0x106, spres << 6);
    chip.begin_beam_line();
    chip.enable_sprites_from_bpl1dat();
    chip.write_sprite_pos(0, 4); // HSTART=8 lores
    chip.write_sprite_ctl(0, ctl);
    chip.write_sprite_datb(0, 0);
    chip.write_sprite_data(0, data);
    let mut pixels = Vec::new();
    for x in 0..32 {
        let sample = chip.output_pixel_with_beam_sprite_coords(x, 0, x, 0, x, 0, true);
        let second = 2;
        pixels.extend([sample.quad_color_idx[0], sample.quad_color_idx[second]]);
    }
    pixels
}

#[test]
fn hires_sprite_serial_bits_and_half_lores_position_are_independent_of_playfield() {
    for playfield in [0, 0x8000, 0x40] {
        for ctl in [0, 0x10] {
            for data in [0xFFFF, 0xA5A5, 0x8001] {
                let got = stream(playfield, 2, ctl, data);
                let mut expected = vec![0; 64];
                let start = 18 + usize::from(ctl != 0);
                for bit in 0..16 {
                    expected[start + bit] = if data & (0x8000 >> bit) != 0 { 17 } else { 0 };
                }
                assert_eq!(
                    got, expected,
                    "PF={playfield:04x}, CTL={ctl:04x}, DATA={data:04x}"
                );
            }
        }
    }
}

#[test]
fn automatic_resolution_retains_lores_for_lores_and_hires_playfields() {
    for playfield in [0, 0x8000] {
        assert_eq!(
            stream(playfield, 0, 0, 0xA5A5),
            stream(playfield, 1, 0, 0xA5A5)
        );
    }
    assert_eq!(stream(0x40, 0, 0, 0xA5A5), stream(0x40, 2, 0, 0xA5A5));
}

#[test]
fn priority_and_collisions_use_simultaneous_hires_sprite_codes() {
    for overlap in [false, true] {
        let mut chip = DeniseAga::new();
        chip.set_bplcon0(0x1000);
        chip.write_word(0x106, 0x80);
        chip.begin_beam_line();
        chip.enable_sprites_from_bpl1dat();
        for sprite in [0, 2] {
            chip.write_sprite_pos(sprite, 4);
            chip.write_sprite_ctl(sprite, 0);
            chip.write_sprite_datb(sprite, 0);
            chip.write_sprite_data(
                sprite,
                if sprite == 0 || overlap {
                    0x8000
                } else {
                    0x4000
                },
            );
        }
        let mut at_start = None;
        for x in 0..12 {
            let sample = chip.output_pixel_with_beam_sprite_coords(x, 0, x, 0, x, 0, true);
            if x == 9 {
                at_start = Some(sample);
            }
        }
        let sample = at_start.expect("capture first visible sprite samples");
        assert_eq!(
            [sample.quad_color_idx[0], sample.quad_color_idx[2]],
            if overlap { [17, 0] } else { [17, 21] }
        );
        assert_eq!(chip.peek_clxdat() & (1 << 9) != 0, overlap);
    }
}

#[test]
fn serial_bits_and_quarter_positions_are_independent_of_playfield() {
    for (spres, period) in [(0x40, 4), (0x80, 2), (0xC0, 1)] {
        for playfield in [0, 0x8000, 0x40] {
            for (ctl, fractional) in [(0, 0), (8, 1), (0x10, 2), (0x18, 3)] {
                for data in [0xFFFF, 0xA5A5, 0x8001] {
                    let mut chip = DeniseAga::new();
                    chip.set_bplcon0(0x1000 | playfield);
                    chip.write_word(0x106, spres);
                    chip.begin_beam_line();
                    chip.enable_sprites_from_bpl1dat();
                    chip.write_sprite_pos(0, 4);
                    chip.write_sprite_ctl(0, ctl);
                    chip.write_sprite_datb(0, 0);
                    chip.write_sprite_data(0, data);
                    let mut got = Vec::new();
                    for x in 0..32 {
                        let sample =
                            chip.output_pixel_with_beam_sprite_coords(x, 0, x, 0, x, 0, true);
                        got.extend(sample.quad_color_idx);
                    }
                    let mut expected = vec![0; 128];
                    for bit in 0..16 {
                        for hold in 0..period {
                            expected[36 + fractional + bit * period + hold] =
                                if data & (0x8000 >> bit) != 0 { 17 } else { 0 };
                        }
                    }
                    assert_eq!(
                        got, expected,
                        "PF={playfield:04x}, CTL={ctl:04x}, DATA={data:04x}"
                    );
                }
            }
        }
    }
}
#[test]
fn collisions_and_priority_distinguish_adjacent_35ns_sprite_bits() {
    for overlap in [false, true] {
        let mut chip = DeniseAga::new();
        chip.set_bplcon0(0x1000);
        chip.write_word(0x106, 0xC0);
        chip.begin_beam_line();
        chip.enable_sprites_from_bpl1dat();
        for sprite in [0, 2] {
            chip.write_sprite_pos(sprite, 4);
            chip.write_sprite_ctl(sprite, 0);
            chip.write_sprite_datb(sprite, 0);
            chip.write_sprite_data(
                sprite,
                if sprite == 0 || overlap {
                    0x8000
                } else {
                    0x4000
                },
            );
        }
        for x in 0..=10 {
            let sample = chip.output_pixel_with_beam_sprite_coords(x, 0, x, 0, x, 0, true);
            if x == 9 {
                assert_eq!(
                    sample.quad_color_idx,
                    if overlap {
                        [17, 0, 0, 0]
                    } else {
                        [17, 21, 0, 0]
                    }
                );
            }
        }
        assert_eq!(chip.peek_clxdat() & (1 << 9) != 0, overlap);
    }
}
