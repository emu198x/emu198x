//! Preserve live Lisa palette selection across runtime serialization.
use commodore_denise_aga::DeniseAga;
use common_commodore_amiga::DeniseChip;

#[test]
fn changing_pf2_offset_preserves_pending_serial_data_and_snapshot_restore() {
    let mut original = DeniseAga::new();
    assert_eq!(original.bplcon3, 0x0C00);
    original.set_bplcon0(0x0410);
    original.begin_beam_line();
    original.load_bitplane(1, 0xFFFF); // PF2 code 1
    original.as_inner_mut().as_inner_mut().trigger_shift_load();
    for x in 0..4 {
        original.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true);
    }
    original.write_word(0x106, 7 << 10);
    let saved = postcard::to_allocvec(&original).expect("save live dual playfield");
    let mut restored: DeniseAga =
        postcard::from_bytes(&saved).expect("restore live dual playfield");
    for x in 4..12 {
        if x == 8 {
            original.write_word(0x106, 1 << 10);
            restored.write_word(0x106, 1 << 10);
        }
        let a = original.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true);
        let b = restored.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true);
        assert_eq!(a, b);
        assert_eq!(a.plane_bits_mask, 2);
        assert_eq!(a.final_color_idx, if x < 8 { 129 } else { 3 });
    }
}
