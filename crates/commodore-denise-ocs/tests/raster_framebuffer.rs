//! Cov-5c — raster framebuffer write paths and viewport extraction.
//!
//! Covers the two pieces that sit on top of the pixel pipeline:
//!   - `write_raster_pixel`: interlace branches, double-row write,
//!     out-of-range coordinates.
//!   - `extract_viewport` (in `chip.rs`) and the viewport helpers in
//!     `viewport.rs` (`pal_bounds` / `ntsc_bounds` / `scale_nearest` /
//!     `to_display` / `pixel_aspect_ratio`).

use commodore_denise_ocs::{
    DeniseOcs, NTSC_RASTER_FB_HEIGHT, PAL_RASTER_FB_HEIGHT, RASTER_FB_WIDTH, ViewportImage,
    ViewportPreset, pixel_aspect_ratio,
};

#[test]
fn write_raster_pixel_non_interlaced_writes_both_rows_of_pair() {
    let mut d = DeniseOcs::new();
    // Pick a CCK and sub well within the standard bounds.
    let hpos = 0x40u16;
    let vpos = 100u16;
    let sub = 3u8;
    d.write_raster_pixel(hpos, vpos, sub, 0xFF11_2233);

    let fb_x = u32::from(hpos) * 8 + u32::from(sub);
    let row_base = u32::from(vpos) * 2;
    let idx0 = (row_base * d.raster_fb_width + fb_x) as usize;
    let idx1 = ((row_base + 1) * d.raster_fb_width + fb_x) as usize;
    assert_eq!(d.framebuffer_raster[idx0], 0xFF11_2233);
    assert_eq!(d.framebuffer_raster[idx1], 0xFF11_2233);
}

#[test]
fn write_raster_pixel_interlaced_long_frame_writes_only_top_row() {
    let mut d = DeniseOcs::new();
    d.interlace_active = true;
    d.lof = true;
    let hpos = 0x40u16;
    let vpos = 100u16;
    let fb_x = u32::from(hpos) * 8;
    let top = (u32::from(vpos) * 2 * d.raster_fb_width + fb_x) as usize;
    let bot = ((u32::from(vpos) * 2 + 1) * d.raster_fb_width + fb_x) as usize;
    let bot_before = d.framebuffer_raster[bot];

    d.write_raster_pixel(hpos, vpos, 0, 0xFF44_5566);
    assert_eq!(d.framebuffer_raster[top], 0xFF44_5566);
    assert_eq!(
        d.framebuffer_raster[bot], bot_before,
        "long-frame interlace should leave the bottom row untouched"
    );
}

#[test]
fn write_raster_pixel_interlaced_short_frame_writes_only_bottom_row() {
    let mut d = DeniseOcs::new();
    d.interlace_active = true;
    d.lof = false;
    let hpos = 0x40u16;
    let vpos = 100u16;
    let fb_x = u32::from(hpos) * 8;
    let top = (u32::from(vpos) * 2 * d.raster_fb_width + fb_x) as usize;
    let bot = ((u32::from(vpos) * 2 + 1) * d.raster_fb_width + fb_x) as usize;
    let top_before = d.framebuffer_raster[top];

    d.write_raster_pixel(hpos, vpos, 0, 0xFF77_8899);
    assert_eq!(d.framebuffer_raster[bot], 0xFF77_8899);
    assert_eq!(
        d.framebuffer_raster[top], top_before,
        "short-frame interlace should leave the top row untouched"
    );
}

#[test]
fn write_raster_pixel_x_out_of_range_is_dropped() {
    let mut d = DeniseOcs::new();
    let hpos = (RASTER_FB_WIDTH / 8) as u16; // exactly past the right edge
    d.write_raster_pixel(hpos, 0, 0, 0xFFFF_FFFF);
    // The framebuffer init colour is 0xFF000000; nothing should have changed.
    assert!(
        d.framebuffer_raster.iter().all(|&v| v == 0xFF000000),
        "out-of-range hpos must not write anywhere"
    );
}

#[test]
fn write_raster_pixel_y_out_of_range_is_dropped_in_both_modes() {
    // Non-interlace: row_base + 1 exceeds height → loop break covers
    // the y-OOB branch in the non-interlaced arm.
    let mut d = DeniseOcs::new();
    let edge_y = (PAL_RASTER_FB_HEIGHT / 2 - 1) as u16; // last valid line pair
    d.write_raster_pixel(0x40, edge_y, 0, 0xFF55_5555);
    let fb_x = 0x40u32 * 8;
    let last_row = ((PAL_RASTER_FB_HEIGHT - 1) * d.raster_fb_width + fb_x) as usize;
    assert_eq!(d.framebuffer_raster[last_row], 0xFF55_5555);

    // One-past-end vpos must be a no-op.
    let oob = (PAL_RASTER_FB_HEIGHT / 2) as u16;
    let before: Vec<u32> = d.framebuffer_raster.clone();
    d.write_raster_pixel(0x40, oob, 0, 0xFFFF_FFFF);
    assert_eq!(
        d.framebuffer_raster, before,
        "out-of-range vpos in non-interlace must not write"
    );

    // Interlace branch with vpos so high it overflows:
    let mut d = DeniseOcs::new();
    d.interlace_active = true;
    let oob = (PAL_RASTER_FB_HEIGHT / 2) as u16;
    let before: Vec<u32> = d.framebuffer_raster.clone();
    d.write_raster_pixel(0x40, oob, 0, 0xFFFF_FFFF);
    assert_eq!(
        d.framebuffer_raster, before,
        "out-of-range vpos in interlace must not write"
    );
}

#[test]
fn extract_viewport_pal_full_matches_raster_fb_height() {
    let d = DeniseOcs::new();
    let img = d.extract_viewport(ViewportPreset::Full, true, false);
    // Full PAL bounds: 227 CCK × 312 lines × 8 hires pixels × 2 rows
    assert_eq!(img.width, 227 * 8);
    assert_eq!(img.height, 312 * 2);
}

#[test]
fn extract_viewport_ntsc_overscan_uses_ntsc_bounds() {
    let d = DeniseOcs::new_with_raster_height(NTSC_RASTER_FB_HEIGHT);
    let img = d.extract_viewport(ViewportPreset::Overscan, false, false);
    let bounds = ViewportPreset::Overscan.ntsc_bounds();
    let expected_w = u32::from(bounds.h_end_cck - bounds.h_start_cck) * 8;
    let expected_h = u32::from(bounds.v_end_line - bounds.v_start_line) * 2;
    assert_eq!(img.width, expected_w);
    assert_eq!(img.height, expected_h);
}

#[test]
fn extract_viewport_deinterlace_halves_height_and_takes_every_other_row() {
    let mut d = DeniseOcs::new();
    // Plant a marker on row 0x32 = (vstart 0x19) × 2 (Standard PAL vstart).
    let bounds = ViewportPreset::Standard.pal_bounds();
    let raster_x = u32::from(bounds.h_start_cck) * 8;
    let raster_row_top = u32::from(bounds.v_start_line) * 2;
    let raster_row_bot = raster_row_top + 1;
    let idx_top = (raster_row_top * d.raster_fb_width + raster_x) as usize;
    let idx_bot = (raster_row_bot * d.raster_fb_width + raster_x) as usize;
    d.framebuffer_raster[idx_top] = 0xFFAA_BBCC;
    d.framebuffer_raster[idx_bot] = 0xFF11_2233;

    let img = d.extract_viewport(ViewportPreset::Standard, true, true);
    let v_lines = u32::from(bounds.v_end_line - bounds.v_start_line);
    assert_eq!(img.height, v_lines, "deinterlace should halve the row count");
    // First pixel of first row should be the top row's marker, not the bottom.
    assert_eq!(img.pixels[0], 0xFFAA_BBCC);
}

#[test]
fn extract_viewport_does_not_oob_when_bounds_exceed_buffer() {
    // Construct with a tiny raster height so Full bounds (312 lines × 2 = 624)
    // mostly fall outside the buffer; the per-pixel `.get(idx).unwrap_or(0xFF000000)`
    // path should yield black for OOB rows.
    let d = DeniseOcs::new_with_raster_height(8);
    let img = d.extract_viewport(ViewportPreset::Full, true, false);
    // First in-range row (raster row 0) was init'd to 0xFF000000.
    assert_eq!(img.pixels[0], 0xFF000000);
    // Far-bottom row is OOB → fallback to 0xFF000000.
    let last_idx = (img.height - 1) * img.width;
    assert_eq!(img.pixels[last_idx as usize], 0xFF000000);
}

#[test]
fn viewport_image_scale_nearest_doubles_in_each_axis() {
    let img = ViewportImage {
        pixels: vec![0xFF000001, 0xFF000002, 0xFF000003, 0xFF000004],
        width: 2,
        height: 2,
    };
    let scaled = img.scale_nearest(4, 4);
    assert_eq!(scaled.width, 4);
    assert_eq!(scaled.height, 4);
    // Top-left 2×2 quadrant should all be 0xFF000001.
    assert_eq!(scaled.pixels[0], 0xFF000001);
    assert_eq!(scaled.pixels[1], 0xFF000001);
    assert_eq!(scaled.pixels[4], 0xFF000001);
    assert_eq!(scaled.pixels[5], 0xFF000001);
    // Top-right 2×2 quadrant -> 0xFF000002
    assert_eq!(scaled.pixels[2], 0xFF000002);
    // Bottom-left -> 0xFF000003
    assert_eq!(scaled.pixels[8], 0xFF000003);
    // Bottom-right -> 0xFF000004
    assert_eq!(scaled.pixels[15], 0xFF000004);
}

#[test]
fn viewport_image_scale_nearest_returns_fallback_for_oob_indices() {
    // An empty image with non-zero target dimensions must produce the
    // 0xFF000000 fallback (covers the `unwrap_or` arm of `scale_nearest`).
    // Use a 1×0 image (height==0 to avoid divide-by-zero on src_y).
    let img = ViewportImage {
        pixels: vec![],
        width: 1,
        height: 0,
    };
    // src_y = (y * 0) / target_h = 0; idx = 0; pixels.get(0) = None → fallback.
    let scaled = img.scale_nearest(2, 1);
    assert_eq!(scaled.pixels, vec![0xFF000000, 0xFF000000]);
}

#[test]
fn viewport_image_to_display_halves_width_doubles_height() {
    let img = ViewportImage {
        pixels: vec![0xFF000000; 16 * 8],
        width: 16,
        height: 8,
    };
    let disp = img.to_display();
    assert_eq!(disp.width, 8);
    assert_eq!(disp.height, 16);
}

#[test]
fn pixel_aspect_ratio_pal_lores_is_16_over_15() {
    let pal_lores = pixel_aspect_ratio(true, false, false);
    assert!((pal_lores - 16.0 / 15.0).abs() < 1e-9);
}

#[test]
fn pixel_aspect_ratio_ntsc_lores_is_8_over_9() {
    let ntsc_lores = pixel_aspect_ratio(false, false, false);
    assert!((ntsc_lores - 8.0 / 9.0).abs() < 1e-9);
}

#[test]
fn pixel_aspect_ratio_hires_halves_horizontal() {
    let pal_hi = pixel_aspect_ratio(true, true, false);
    assert!((pal_hi - 16.0 / 15.0 * 0.5).abs() < 1e-9);
}

#[test]
fn pixel_aspect_ratio_interlace_halves_vertical() {
    let pal_lace = pixel_aspect_ratio(true, false, true);
    assert!((pal_lace - 16.0 / 15.0 * 0.5).abs() < 1e-9);
}

#[test]
fn viewport_preset_pal_and_ntsc_full_bounds_match_raster_size() {
    let pal_full = ViewportPreset::Full.pal_bounds();
    assert_eq!(pal_full.h_end_cck - pal_full.h_start_cck, 0xE3);
    assert_eq!(pal_full.v_end_line - pal_full.v_start_line, 312);

    let ntsc_full = ViewportPreset::Full.ntsc_bounds();
    assert_eq!(ntsc_full.h_end_cck - ntsc_full.h_start_cck, 0xE3);
    assert_eq!(ntsc_full.v_end_line - ntsc_full.v_start_line, 262);
}

#[test]
fn viewport_preset_pal_overscan_bounds_have_documented_centring() {
    let bounds = ViewportPreset::Overscan.pal_bounds();
    assert_eq!(bounds.h_start_cck, 0x1C);
    assert_eq!(bounds.h_end_cck, 0xDC);
    assert_eq!(bounds.v_start_line, 0x1A);
    assert_eq!(bounds.v_end_line, 0x138);
}

#[test]
fn viewport_preset_ntsc_standard_bounds_match_4_3_240_lines() {
    let bounds = ViewportPreset::Standard.ntsc_bounds();
    assert_eq!(bounds.h_start_cck, 0x3C);
    assert_eq!(bounds.h_end_cck, 0xDC); // 160 CCKs
    assert_eq!(bounds.v_start_line, 0x10);
    assert_eq!(bounds.v_end_line, 0x100); // 240 lines
}

#[test]
fn denise_default_constructs_via_default_trait() {
    // Exercises the `impl Default for DeniseOcs` arm. Default should
    // produce the same shape as `DeniseOcs::new()`.
    let d: DeniseOcs = Default::default();
    assert_eq!(d.raster_fb_height, PAL_RASTER_FB_HEIGHT);
    assert_eq!(d.raster_fb_width, RASTER_FB_WIDTH);
}
