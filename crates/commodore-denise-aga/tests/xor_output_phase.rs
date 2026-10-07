//! BPLAM phase qualified by the reference counter, not its padded storage origin.
//! Early RGA updates at counter 260; XOR becomes visible at 261.5 (six samples).
use commodore_denise_aga::DeniseAga;
use common_commodore_amiga::DeniseChip;

#[test]
fn playfield_xor_crosses_the_output_stage_six_native_samples_after_the_write() {
    let mut chip = DeniseAga::new();
    chip.set_bplcon0(0x1000);
    chip.palette_24[0] = 0x000011;
    chip.palette_24[1] = 0x00FF00;
    chip.begin_beam_line();
    chip.write_word(0x110, 0);
    let mut x = 0;
    for _ in 0..20 {
        let output = chip.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true);
        for sample in 0..4 {
            chip.resolve_output_sample_argb(
                output.quad_playfield_color_idx[sample],
                output.quad_color_idx[sample],
                output.quad_is_sprite[sample],
                sample as u8,
            );
        }
        x += 1;
    }
    chip.write_word(0x10C, 0x0111);
    assert_eq!(chip.bplcon4, 0x0111, "raw mirror updates immediately");
    for tick in 0..4 {
        let offset = tick * 4;
        let output = chip.output_pixel_with_beam_and_playfield_gate(x + tick, 0, x + tick, 0, true);
        for sample in 0..4 {
            let rgb = chip.resolve_output_sample_argb(
                output.quad_playfield_color_idx[sample],
                output.quad_color_idx[sample],
                false,
                sample as u8,
            );
            assert_eq!(
                rgb,
                if offset + (sample as u32) < 6 {
                    0xFF00_0011
                } else {
                    0xFF00_FF00
                },
                "native offset {}",
                offset + sample as u32
            );
        }
    }
}

#[test]
fn pending_playfield_xor_does_not_recolour_the_border() {
    let mut chip = DeniseAga::new();
    chip.palette_24[0] = 0x123456;
    chip.palette_24[1] = 0xABCDEF;
    chip.write_word(0x10C, 0x0111);
    for x in 0..8 {
        let output = chip.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, false);
        for sample in 0..4 {
            assert_eq!(
                chip.resolve_output_sample_argb(
                    output.quad_playfield_color_idx[sample],
                    output.quad_color_idx[sample],
                    false,
                    sample as u8
                ),
                0xFF12_3456
            );
        }
    }
}
