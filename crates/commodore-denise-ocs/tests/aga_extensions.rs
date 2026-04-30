//! Cov-5c — AGA-only extension paths exposed by the Denise core.
//!
//! Even though the crate is named `commodore-denise-ocs`, the chip core
//! includes paths that only fire when the outer chipset wrapper sets:
//!
//!   - `max_bitplanes > 6` (AGA 8-plane mode, BPLCON0 BPU=4 bits)
//!   - SHRES (BPLCON0 bit 6 — super-hires, 4 source pixels per call)
//!   - FMODE-driven wider bitplane fetches (`push_bpl_fifo` queue)
//!   - `spr_width` > 16 (32 / 64 px AGA sprites)
//!
//! `machine-commodore-amiga-ocs` does not currently exercise these
//! today (no AGA wrapper in tree), so the chip core is the only place
//! they get tested. When a future ECS/AGA wrapper lands these tests act
//! as regression coverage for the dormant paths.

use commodore_denise_ocs::DeniseOcs;

#[test]
fn num_bitplanes_aga_decodes_4_bit_bpu_via_max_bitplanes_8() {
    let mut d = DeniseOcs::new();
    d.max_bitplanes = 8;

    // BPU hi3 = 0b101 (5), bit 4 = 0 → bpu = 5
    d.bplcon0 = 5u16 << 12; // 0x5000
    assert_eq!(d.num_bitplanes(), 5);

    // BPU hi3 = 0b000, bit 4 = 1 → bpu = 8 (capped at max_bitplanes)
    d.bplcon0 = 1u16 << 4;
    assert_eq!(d.num_bitplanes(), 8);

    // BPU hi3 = 0b011 (3), bit 4 = 1 → bpu = 8 + 3 = 11, capped to 8
    d.bplcon0 = (3u16 << 12) | (1u16 << 4);
    assert_eq!(d.num_bitplanes(), 8, "AGA BPU should be capped at max_bitplanes");
}

#[test]
fn num_bitplanes_ocs_ignores_bplcon0_bit_4() {
    // With max_bitplanes=6 (OCS default), bit 4 must not extend BPU.
    let mut d = DeniseOcs::new();
    d.bplcon0 = (3u16 << 12) | (1u16 << 4); // hi3=3, bit4=1
    assert_eq!(d.num_bitplanes(), 3, "OCS path takes only the 3-bit BPU field");
}

#[test]
fn shres_consumes_four_source_pixels_per_output_call() {
    // BPLCON0 bit 6 = SHRES → source_pixels_per_output_call = 4.
    let mut d = DeniseOcs::new();
    d.bplcon0 = 0x1040; // SHRES + BPU=1 (HIRES bit 15 unset to take the SHRES branch)
    d.set_palette(0, 0x000);
    d.set_palette(1, 0xFFF);
    d.begin_beam_line();
    d.bpl_data[0] = 0xF000; // bits: 1,1,1,1, 0,0,0,0, ...
    d.trigger_shift_load();

    let dbg = d.output_pixel_with_beam(0, 0, 0, 0);
    assert_eq!(
        dbg.source_pixels_per_fb_pixel, 4,
        "SHRES should consume 4 source pixels per output call"
    );
    assert_eq!(d.shift_count, 12, "SHRES drains 4 from the 16-pixel register");
}

#[test]
fn push_bpl_fifo_drives_auto_reload_after_first_word_drains() {
    // Push two distinct words into the FIFO, then drive the regular
    // shifter empty. When the first 16-bit shift register drains, the
    // plane should auto-reload from the FIFO (popping 0x1234 first,
    // then 0xBEEF on the next reload).
    let mut d = DeniseOcs::new();
    d.bplcon0 = 0x1000; // LORES, BPU=1
    d.set_palette(0, 0x000);
    d.set_palette(1, 0xFFF);
    d.begin_beam_line();
    d.bpl_data[0] = 0x0000; // first word in regular shifter -> all zeros
    d.trigger_shift_load();
    d.push_bpl_fifo(0, 0x8000); // 1st FIFO word: MSB set -> pixel = 1
    d.push_bpl_fifo(0, 0x4000); // 2nd FIFO word: bit 14 -> pixel = 0,1,0,...

    // Drain the original 16 zero pixels.
    for x in 0..16u32 {
        let dbg = d.output_pixel_with_beam(x, 0, x, 0);
        assert_eq!(dbg.final_color_idx, 0, "drain initial zero word, x={x}");
    }
    // Pixel 16 should pop the first FIFO word (0x8000) and emit 1.
    let dbg = d.output_pixel_with_beam(16, 0, 16, 0);
    assert_eq!(
        dbg.final_color_idx, 1,
        "FIFO auto-reload should make 1st queued word visible"
    );
    // Drain the rest of the first FIFO word (15 zeros).
    for x in 17..32u32 {
        let _ = d.output_pixel_with_beam(x, 0, x, 0);
    }
    // Pixel 32 should pop the second FIFO word (0x4000) -> first pixel = 0.
    let dbg = d.output_pixel_with_beam(32, 0, 32, 0);
    assert_eq!(dbg.final_color_idx, 0);
    let dbg = d.output_pixel_with_beam(33, 0, 33, 0);
    assert_eq!(
        dbg.final_color_idx, 1,
        "second FIFO pop should bring 0x4000's bit 14 into view"
    );
}

#[test]
fn push_bpl_fifo_caps_at_four_entries() {
    // Push 6 words; the FIFO should silently discard the last 2.
    // Verify by draining: only the first 4 words should be visible
    // through the auto-reload path.
    let mut d = DeniseOcs::new();
    d.bplcon0 = 0x1000;
    d.set_palette(0, 0x000);
    d.set_palette(1, 0xFFF);
    d.begin_beam_line();
    d.bpl_data[0] = 0x0000;
    d.trigger_shift_load();
    // All four accepted FIFO words have MSB set -> first pixel after
    // each reload should be 1. The two over-cap pushes (0x8000 again)
    // are silently dropped.
    for _ in 0..6 {
        d.push_bpl_fifo(0, 0x8000);
    }

    // Drain the first 16 zero pixels.
    for x in 0..16u32 {
        let _ = d.output_pixel_with_beam(x, 0, x, 0);
    }
    // After 4 FIFO reloads (4 × 16 = 64 pixels) the shift_count should
    // hit 0 and stay there — the over-cap entries were dropped.
    for _ in 0..64 {
        let _ = d.output_pixel_with_beam(0, 0, 0, 0);
    }
    assert_eq!(d.shift_count, 0, "FIFO should expose at most 4 reloads");
}

#[test]
fn push_bpl_fifo_ignores_idx_8_or_higher() {
    // Out-of-range plane index is a no-op; verify the plane-0 FIFO is
    // untouched by an idx=8 push.
    let mut d = DeniseOcs::new();
    d.bplcon0 = 0x1000;
    d.set_palette(0, 0x000);
    d.set_palette(1, 0xFFF);
    d.begin_beam_line();
    d.bpl_data[0] = 0x0000;
    d.trigger_shift_load();
    d.push_bpl_fifo(8, 0xDEAD); // ignored — must not appear later
    d.push_bpl_fifo(0, 0x8000); // accepted

    // Drain initial zero word.
    for x in 0..16u32 {
        let _ = d.output_pixel_with_beam(x, 0, x, 0);
    }
    // First FIFO entry (and only one) is the 0x8000 — its MSB drives pixel 16.
    let dbg = d.output_pixel_with_beam(16, 0, 16, 0);
    assert_eq!(dbg.final_color_idx, 1);
}

#[test]
fn load_bitplane_ignores_idx_8_or_higher() {
    let mut d = DeniseOcs::new();
    d.load_bitplane(8, 0xDEAD); // ignored
    d.load_bitplane(7, 0xBEEF); // last valid plane
    assert_eq!(d.bpl_data[7], 0xBEEF);
}

#[test]
fn rgb12_to_rgb24_replicates_each_nibble() {
    // 0xABC → R=A, G=B, B=C with each nibble doubled into a byte.
    let val = DeniseOcs::rgb12_to_rgb24(0xABC);
    assert_eq!(val, 0x00AABBCC);
    assert_eq!(DeniseOcs::rgb12_to_rgb24(0x000), 0);
    assert_eq!(DeniseOcs::rgb12_to_rgb24(0xFFF), 0x00FFFFFF);
}

#[test]
fn rgb24_to_argb32_sets_alpha_to_ff() {
    assert_eq!(DeniseOcs::rgb24_to_argb32(0), 0xFF000000);
    assert_eq!(DeniseOcs::rgb24_to_argb32(0x00112233), 0xFF112233);
}

#[test]
fn defer_shift_load_with_count_zero_triggers_immediate_load() {
    // count == 0 should fall through to `trigger_shift_load` directly.
    let mut d = DeniseOcs::new();
    d.bplcon0 = 0x1000;
    d.set_palette(0, 0x000);
    d.set_palette(1, 0xFFF);
    d.begin_beam_line();
    d.bpl_data[0] = 0x8000;
    d.defer_shift_load_after_source_pixels(0);

    // Shift register should already be loaded; no deferral pending.
    let dbg = d.output_pixel_with_beam(0, 0, 0, 0);
    assert_eq!(dbg.final_color_idx, 1, "count=0 should load immediately");
}

#[test]
fn defer_shift_load_with_count_greater_than_one_decrements_per_pixel() {
    // count=3: load fires after 3 source pixels are consumed.
    let mut d = DeniseOcs::new();
    d.bplcon0 = 0x1000;
    d.set_palette(0, 0x000);
    d.set_palette(1, 0xFFF);
    d.begin_beam_line();
    d.bpl_data[0] = 0x0000;
    d.trigger_shift_load();
    d.bpl_data[0] = 0x8000; // queued for the deferred load
    d.defer_shift_load_after_source_pixels(3);

    // Pixels 0, 1, 2 see the original (zero) data; pixel 3 sees the new.
    for x in 0..3u32 {
        let dbg = d.output_pixel_with_beam(x, 0, x, 0);
        assert_eq!(dbg.final_color_idx, 0, "pixel {x} pre-deferred-load");
    }
    let dbg = d.output_pixel_with_beam(3, 0, 3, 0);
    assert_eq!(dbg.final_color_idx, 1, "deferred load should fire on pixel 3");
}
