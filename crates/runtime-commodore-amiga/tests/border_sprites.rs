//! Restore Lisa border eligibility with a sprite in its retained output stage.
use commodore_denise_aga::DeniseAga;
use common_commodore_amiga::DeniseChip;

#[test]
fn border_sprite_resumes_without_a_bitplane_arrival() {
    let mut original = DeniseAga::new();
    original.set_bplcon0(1);
    original.write_word(0x106, 2);
    original.begin_beam_line();
    original.write_sprite_pos(0, 0);
    original.write_sprite_ctl(0, 0);
    original.write_sprite_datb(0, 0);
    original.write_sprite_data(0, 0xFFFF);
    for x in 0..2 {
        original.output_pixel_with_beam_sprite_coords(x, 0, x, 0, x, 0, false);
    }
    let bytes = postcard::to_allocvec(&original).expect("save pending border sprite");
    let mut restored: DeniseAga = postcard::from_bytes(&bytes).expect("restore border sprite");
    for x in 2..6 {
        let a = original.output_pixel_with_beam_sprite_coords(x, 0, x, 0, x, 0, false);
        let b = restored.output_pixel_with_beam_sprite_coords(x, 0, x, 0, x, 0, false);
        assert_eq!(a, b);
        assert!(a.quad_is_sprite[0]);
        assert_eq!(a.final_color_idx, 17);
        assert!(!restored.as_inner().as_inner().sprite_bpl1dat_enabled());
        assert_eq!(restored.read_clxdat(), original.read_clxdat());
    }
}
