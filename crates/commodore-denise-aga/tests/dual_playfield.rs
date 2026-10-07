//! Lisa PF2OF address selection, independently specified by WinUAE's
//! dblpfofs[] and Minimig's denise_playfields.v lookup table.
use commodore_denise_aga::DeniseAga;
use common_commodore_amiga::DeniseChip;

const OFFSETS: [u8; 8] = [0, 2, 4, 8, 16, 32, 64, 128];

fn pixel(pf1: u8, pf2: u8, selector: u8, pf2_front: bool, xor: u8, resolution: u16) -> u8 {
    let mut denise = DeniseAga::new();
    denise.set_bplcon0(0x0410 | resolution); // eight planes, dual playfield
    denise.write_word(0x106, u16::from(selector) << 10);
    denise.write_word(0x104, if pf2_front { 0x40 } else { 0 });
    denise.write_word(0x10C, (u16::from(xor) << 8) | 0x11);
    denise.begin_beam_line();
    for plane in 0..8 {
        let code = if plane & 1 == 0 { pf1 } else { pf2 };
        denise.load_bitplane(
            plane,
            if code & (1 << (plane / 2)) != 0 {
                0xFFFF
            } else {
                0
            },
        );
    }
    denise.as_inner_mut().as_inner_mut().trigger_shift_load();
    for x in 0..=3 {
        let sample = denise.output_pixel_with_beam_sprite_coords(x, 0, x, 0, x + 40, 0, true);
        if x == 3 {
            let count = usize::from(sample.output_samples_per_fb_pixel);
            for index in &sample.quad_playfield_color_idx[..count] {
                assert_eq!(*index, sample.final_color_idx);
            }
            assert!(!sample.quad_is_sprite[0]);
            return sample.final_color_idx;
        }
    }
    unreachable!()
}

#[test]
fn all_pf2_offsets_apply_after_transparency_and_priority_before_xor() {
    for (selector, offset) in OFFSETS.into_iter().enumerate() {
        for pf1 in [0, 8, 15] {
            for pf2 in 0..16 {
                for pf2_front in [false, true] {
                    for xor in [0, 0xA5] {
                        let selected = if pf2 != 0 && (pf1 == 0 || pf2_front) {
                            pf2 + offset
                        } else {
                            pf1
                        };
                        assert_eq!(
                            pixel(pf1, pf2, selector as u8, pf2_front, xor, 0),
                            selected ^ xor,
                            "PF1={pf1}, PF2={pf2}, PF2OF={selector}, PF2PRI={pf2_front}, BPLAM={xor:02x}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn pf2_offsets_reach_each_hires_and_superhires_sample() {
    for resolution in [0x8000, 0x0040] {
        for (selector, offset) in OFFSETS.into_iter().enumerate() {
            assert_eq!(
                pixel(0, 15, selector as u8, true, 0, resolution),
                15 + offset
            );
            let mut denise = DeniseAga::new();
            denise.set_bplcon0(0x0410 | resolution);
            denise.write_word(0x106, (selector as u16) << 10);
            denise.write_word(0x10C, 0xA511);
            denise.begin_beam_line();
            denise.load_bitplane(1, 0xAAAA); // alternate PF2 code 1 / transparent
            denise.as_inner_mut().as_inner_mut().trigger_shift_load();
            denise.output_pixel_with_beam_and_playfield_gate(0, 0, 0, 0, true);
            let sample = denise.output_pixel_with_beam_and_playfield_gate(1, 0, 1, 0, true);
            for i in 0..usize::from(sample.source_pixels_per_fb_pixel) {
                let code = if i & 1 == 0 { 1 } else { 0 };
                assert_eq!(sample.quad_samples[i].pf2_code, code);
                let palette = if code == 0 { 0 } else { offset + code };
                let repeat = usize::from(sample.output_samples_per_fb_pixel)
                    / usize::from(sample.source_pixels_per_fb_pixel);
                for output in i * repeat..(i + 1) * repeat {
                    assert_eq!(sample.quad_playfield_color_idx[output], palette ^ 0xA5);
                    assert_eq!(sample.quad_color_idx[output], palette ^ 0xA5);
                }
            }
        }
    }
}

#[test]
fn ocs_and_ecs_keep_the_fixed_playfield_two_offset() {
    fn emit<D: DeniseChip>(mut denise: D, selector: u16) {
        denise.write_word(0x106, selector << 10);
        for x in 0..5 {
            let sample = denise.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true);
            if x == 4 {
                assert_eq!(sample.final_color_idx, 15);
            }
        }
    }
    for selector in 0..8 {
        let mut ocs = commodore_denise_ocs::DeniseOcs::new();
        ocs.bplcon0 = 0x6400;
        ocs.begin_beam_line();
        for plane in [1, 3, 5] {
            ocs.load_bitplane(plane, 0xFFFF);
        }
        ocs.trigger_shift_load();
        emit(
            commodore_denise_ecs::DeniseEcs::from_ocs(ocs.clone()),
            selector,
        );
        emit(ocs, selector);
    }
}
