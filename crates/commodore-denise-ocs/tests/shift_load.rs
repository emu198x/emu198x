//! Bitplane shift-register load timing — fine-grained behaviour of the
//! `output_pixel_with_beam` pipeline that complements the broader
//! BPLCON pipeline coverage in `bplcon_pipeline.rs`.
//!
//! Where `bplcon_pipeline.rs` characterises the lores/hires source-pixel
//! rates against single-call outputs, this file verifies:
//!   - exact `shift_count` decrement after one output call
//!   - that lores leaves `quad_samples[1..]` at default
//!   - cumulative shifting across two consecutive hires output calls
//!   - the deferred-shift-load hook firing between hires source samples

use commodore_denise_ocs::{DeniseOcs, DeniseSourcePixelDebug};

fn invisible_output_pixel(
    denise: &mut DeniseOcs,
    beam_x: u32,
) -> commodore_denise_ocs::DeniseOutputPixelDebug {
    denise.output_pixel_with_beam(u32::MAX, u32::MAX, beam_x, 0)
}

#[test]
fn lowres_output_pixel_with_beam_consumes_one_source_pixel_per_call() {
    let mut denise = DeniseOcs::new();
    denise.bplcon0 = 0x1000; // 1 bitplane, lowres
    denise.begin_beam_line();
    denise.bpl_data[0] = 0xA000; // bits: 1,0,1,0,...
    denise.trigger_shift_load();

    let dbg = invisible_output_pixel(&mut denise, 0);

    assert!(dbg.called);
    assert!(!dbg.hires);
    assert_eq!(dbg.source_pixels_per_fb_pixel, 1);
    assert_eq!(dbg.quad_samples[0].raw_color_idx, 1);
    assert_eq!(
        dbg.quad_samples[1],
        DeniseSourcePixelDebug::default(),
        "lowres path should not consume a second source pixel in the same call"
    );
    assert_eq!(denise.shift_count, 15);
}

#[test]
fn hires_output_pixel_with_beam_consumes_two_source_pixels_per_call() {
    let mut denise = DeniseOcs::new();
    denise.bplcon0 = 0x9000; // HIRES + 1 bitplane
    denise.begin_beam_line();
    denise.bpl_data[0] = 0xC000; // bits: 1,1,0,0,...
    denise.trigger_shift_load();

    let dbg = invisible_output_pixel(&mut denise, 0);

    assert!(dbg.called);
    assert!(dbg.hires);
    assert_eq!(dbg.source_pixels_per_fb_pixel, 2);
    assert_eq!(dbg.quad_samples[0].raw_color_idx, 1);
    assert_eq!(dbg.quad_samples[1].raw_color_idx, 1);
    assert_eq!(denise.shift_count, 14);
}

#[test]
fn two_hires_output_calls_advance_four_source_pixels_total() {
    let mut denise = DeniseOcs::new();
    denise.bplcon0 = 0x9000; // HIRES + 1 bitplane
    denise.begin_beam_line();
    denise.bpl_data[0] = 0xA000; // bits: 1,0,1,0,...
    denise.trigger_shift_load();

    let dbg0 = invisible_output_pixel(&mut denise, 0);
    let dbg1 = invisible_output_pixel(&mut denise, 1);

    // Full-rate shift: each output call consumes 2 distinct source pixels.
    // 0xA000 = 1010_0000... so pixels are: 1, 0, 1, 0, ...
    assert_eq!(
        [
            dbg0.quad_samples[0].raw_color_idx,
            dbg0.quad_samples[1].raw_color_idx
        ],
        [1, 0],
        "first output call shifts source pixels 0 (=1) and 1 (=0)"
    );
    assert_eq!(
        [
            dbg1.quad_samples[0].raw_color_idx,
            dbg1.quad_samples[1].raw_color_idx
        ],
        [1, 0],
        "second output call shifts source pixels 2 (=1) and 3 (=0)"
    );
    assert_eq!(denise.shift_count, 12);
}

#[test]
fn deferred_shift_load_lands_between_hires_samples_in_one_call() {
    let mut denise = DeniseOcs::new();
    denise.bplcon0 = 0x9000; // HIRES + 1 bitplane
    denise.begin_beam_line();
    denise.bpl_data[0] = 0x0000;
    denise.trigger_shift_load();

    denise.bpl_data[0] = 0x8000; // next fetched word
    denise.defer_shift_load_after_source_pixels(1);

    // Full-rate shift: first source pixel (0) triggers the deferred load,
    // second source pixel is bit 15 of the new word (1).
    let dbg = invisible_output_pixel(&mut denise, 0);
    assert_eq!(
        [
            dbg.quad_samples[0].raw_color_idx,
            dbg.quad_samples[1].raw_color_idx
        ],
        [0, 1],
        "deferred load fires after first shift, second shift sees new data"
    );
}
