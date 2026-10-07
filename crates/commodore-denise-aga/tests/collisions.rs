//! Collision truth tables from Minimig denise_collision.v and WinUAE
//! expand_colmask(): BP7/BP8 extend the odd/even comparisons before latching.
use commodore_denise_aga::DeniseAga;
use common_commodore_amiga::DeniseChip;

fn fixture(mask: u8, clxcon: u16, extension: u16) -> DeniseAga {
    let mut denise = DeniseAga::new();
    denise.set_bplcon0(0x0410); // eight lores planes, independent dual-playfield matches
    denise.write_word(0x104, 0x0024); // place sprite groups in front of both playfields
    denise.write_word(0x098, clxcon);
    denise.write_word(0x10E, extension);
    denise.begin_beam_line();
    for sprite in [0, 2] {
        denise.write_sprite_pos(sprite, 0);
        denise.write_sprite_ctl(sprite, 0);
        denise.write_sprite_datb(sprite, 0);
        denise.write_sprite_data(sprite, 0xFFFF);
    }
    for plane in 0..8 {
        denise.load_bitplane(plane, if mask & (1 << plane) == 0 { 0 } else { 0xFFFF });
    }
    denise.queue_shift_load_from_bpl1dat();
    denise.as_inner_mut().as_inner_mut().trigger_shift_load();
    for x in 0..2 {
        denise.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true);
        denise.read_clxdat();
    }
    denise
}

fn emit_and_read(denise: &mut DeniseAga, x: u32, mask: u8) -> u16 {
    let pixel = denise.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true);
    assert_eq!(
        pixel.plane_bits_mask, mask,
        "fixture must emit its actual plane bits"
    );
    assert!(
        pixel.quad_is_sprite[0],
        "collision fixture must have a visible sprite"
    );
    denise.read_clxdat()
}

#[test]
fn seventh_and_eighth_planes_extend_independent_collision_truth_tables() {
    for mask in [0, 0x40, 0x80, 0xC0] {
        for enabled in 0..4u16 {
            for expected in 0..4u16 {
                let extension = (enabled << 6) | expected;
                let mut denise = fixture(mask, 0, extension);
                let collision = emit_and_read(&mut denise, 2, mask);
                let odd = enabled & 1 == 0 || (mask & 0x40 != 0) == (expected & 1 != 0);
                let even = enabled & 2 == 0 || (mask & 0x80 != 0) == (expected & 2 != 0);
                let wanted = u16::from(odd && even)
                    | if odd { (1 << 1) | (1 << 2) } else { 0 }
                    | if even { (1 << 5) | (1 << 6) } else { 0 }
                    | (1 << 9); // sprite/sprite collisions are independent
                assert_eq!(
                    collision, wanted,
                    "mask={mask:02x}, CLXCON2={extension:04x}"
                );
                assert_eq!(denise.read_clxdat(), 0, "CLXDAT still reads and clears");
            }
        }
    }
}

#[test]
fn base_register_write_clears_extension_without_clearing_latched_collisions() {
    let mut denise = fixture(0, 0, 0xC3); // both extra planes require a one
    assert_eq!(emit_and_read(&mut denise, 2, 0), 1 << 9);
    denise.write_word(0x098, 0); // clears extra plane enables
    let all = emit_and_read(&mut denise, 3, 0);
    assert_eq!(all, 0x0267);
    denise.output_pixel_with_beam_and_playfield_gate(4, 0, 4, 0, true);
    denise.write_word(0x10E, 0xC3);
    denise.write_word(0x098, 0);
    assert_eq!(
        denise.read_clxdat(),
        all,
        "register writes must retain existing collision bits"
    );
}

#[test]
fn disabled_and_reserved_extension_bits_do_not_override_the_base_planes() {
    let mut denise = fixture(0, 0x0041, 0xFF3F); // BP1 requires one; extra enables clear
    assert_eq!(
        emit_and_read(&mut denise, 2, 0),
        (1 << 5) | (1 << 6) | (1 << 9)
    );
    let mut denise = fixture(0x41, 0x0041, 0xFF7D); // BP1 and BP7 require one
    assert_eq!(emit_and_read(&mut denise, 2, 0x41), 0x0267);
}

#[test]
fn hires_pixels_cannot_manufacture_a_simultaneous_playfield_collision() {
    let mut denise = fixture(0, 0x00C3, 0); // BP1=BP2=1 required
    denise.begin_beam_line();
    denise.set_bplcon0(0x8010); // eight hires planes
    denise.load_bitplane(0, 0xAAAA);
    denise.load_bitplane(1, 0x5555); // each pixel has only one plane set
    denise.queue_shift_load_from_bpl1dat();
    denise.as_inner_mut().as_inner_mut().trigger_shift_load();
    denise.output_pixel_with_beam_and_playfield_gate(0, 0, 0, 0, true);
    denise.read_clxdat();
    let pixel = denise.output_pixel_with_beam_and_playfield_gate(1, 0, 1, 0, true);
    assert_eq!(
        [
            pixel.quad_samples[0].raw_color_idx,
            pixel.quad_samples[1].raw_color_idx
        ],
        [1, 2]
    );
    assert_eq!(
        denise.read_clxdat() & 1,
        0,
        "two different hires pixels never simultaneously satisfy BP1 and BP2"
    );
}

#[test]
fn single_playfield_odd_collision_requires_the_even_planes_to_match() {
    let mut dual = fixture(0xC0, 0, 0xC1); // BP7 matches, BP8 does not
    assert_eq!(
        emit_and_read(&mut dual, 2, 0xC0),
        (1 << 1) | (1 << 2) | (1 << 9)
    );
    let mut single = fixture(0xC0, 0, 0xC1);
    single.set_bplcon0(0x0010);
    assert_eq!(emit_and_read(&mut single, 2, 0xC0), 1 << 9);
}

#[test]
fn hires_and_superhires_extra_planes_match_each_actual_sample_including_zero() {
    for mode in [0x8410, 0x0450] {
        for extension in [0xC0, 0xC3] {
            let mut denise = fixture(0, 0, extension);
            denise.begin_beam_line();
            denise.set_bplcon0(mode);
            denise.load_bitplane(6, 0xAAAA);
            denise.load_bitplane(7, 0x5555);
            denise.queue_shift_load_from_bpl1dat();
            denise.as_inner_mut().as_inner_mut().trigger_shift_load();
            for x in 0..2 {
                denise.output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true);
                denise.read_clxdat();
            }
            let pixel = denise.output_pixel_with_beam_and_playfield_gate(2, 0, 2, 0, true);
            assert_eq!(
                [
                    pixel.quad_samples[0].raw_color_idx,
                    pixel.quad_samples[1].raw_color_idx
                ],
                [0x40, 0x80]
            );
            assert_eq!(
                denise.read_clxdat(),
                0x0266,
                "mode={mode:04x}, extension={extension:04x}: both groups match at different source pixels; never simultaneously"
            );
        }
    }
}

#[test]
fn ocs_and_ecs_do_not_decode_the_aga_extension() {
    let mut ocs = commodore_denise_ocs::DeniseOcs::new();
    ocs.write_word(0x10E, 0x00C3);
    assert_eq!(ocs.diagnostic_snapshot().clxcon2, 0);
    let mut ecs = commodore_denise_ecs::DeniseEcs::new();
    ecs.write_word(0x10E, 0x00C3);
    assert_eq!(ecs.as_inner().diagnostic_snapshot().clxcon2, 0);
}

#[test]
fn manual_bpl8dat_write_reaches_the_eighth_shifter_on_bpl1dat_strobe() {
    let mut denise = DeniseAga::new();
    denise.set_bplcon0(0x0010); // eight lores bitplanes
    denise.begin_beam_line();
    denise.write_word(0x011E, 0x8000); // BPL8DAT must not strobe the parallel copy
    let holding = denise.as_inner().as_inner().diagnostic_snapshot().bitplanes;
    assert_eq!(holding.holding_data[7], 0x8000);
    assert!(!holding.pending_copy_even_planes);
    denise.write_word(0x0110, 0); // BPL1DAT strobes all eight holding registers
    let outputs: Vec<_> = (0..4)
        .map(|x| {
            denise
                .output_pixel_with_beam_and_playfield_gate(x, 0, x, 0, true)
                .quad_playfield_color_idx[0]
        })
        .collect();
    assert_eq!(
        outputs,
        [0, 128, 0, 0],
        "manual eighth-plane data must reach real source output"
    );
}
