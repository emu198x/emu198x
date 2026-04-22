//! Phase 1 characterization tests — Agnus beam counter + frame timing.
//!
//! Per HRM Chapter 6 (Display Hardware) and Appendix A (Beam Counter).
//! PAL non-interlaced: every line is 227 CCKs, every frame is 312
//! lines → 70,824 CCKs per frame at 3.546895 MHz = 50.0786 Hz fields.
//! NTSC non-interlaced: 227 or 228 CCKs/line (long-line alternation
//! not yet modelled), 262 lines per frame.
//!
//! Interlace (BPLCON0.LACE) adds a 313th line to odd ("long") PAL
//! frames — LOF bit in VPOSR tracks which. Non-interlace frames are
//! always 312 lines.

use commodore_agnus_ocs::{Agnus, PAL_CCKS_PER_LINE, PAL_LINES_PER_FRAME};

const PAL_CCKS_PER_FRAME: u64 = PAL_CCKS_PER_LINE as u64 * PAL_LINES_PER_FRAME as u64;

#[test]
fn pal_constants_match_hardware_reference() {
    assert_eq!(PAL_CCKS_PER_LINE, 227);
    assert_eq!(PAL_LINES_PER_FRAME, 312);
    assert_eq!(PAL_CCKS_PER_FRAME, 70_824);
}

#[test]
fn tick_advances_hpos_then_wraps_to_vpos() {
    let mut a = Agnus::new();
    for _ in 0..PAL_CCKS_PER_LINE {
        a.tick_cck();
    }
    assert_eq!(a.hpos, 0);
    assert_eq!(a.vpos, 1);
}

#[test]
fn vpos_wraps_at_end_of_frame_non_interlace() {
    let mut a = Agnus::new();
    for _ in 0..PAL_CCKS_PER_FRAME {
        a.tick_cck();
    }
    assert_eq!(a.vpos, 0);
    assert_eq!(a.hpos, 0);
}

#[test]
fn ten_frames_traverses_exactly_ten_frame_periods() {
    let mut a = Agnus::new();
    for _ in 0..(10 * PAL_CCKS_PER_FRAME) {
        a.tick_cck();
    }
    assert_eq!(a.vpos, 0);
    assert_eq!(a.hpos, 0);
}

#[test]
fn mid_frame_position_is_reachable_and_correct() {
    let mut a = Agnus::new();
    // Beam at vpos=50, hpos=42.
    let ticks = 50 * u64::from(PAL_CCKS_PER_LINE) + 42;
    for _ in 0..ticks {
        a.tick_cck();
    }
    assert_eq!(a.vpos, 50);
    assert_eq!(a.hpos, 42);
}

// ─── Interlace / LOF ───────────────────────────────────────────────

#[test]
fn interlace_toggles_lof_every_frame_and_long_frame_has_extra_line() {
    let mut a = Agnus::new();
    a.bplcon0 = 0x0004; // LACE enabled
    assert!(a.lof, "starts in long (odd) frame");

    // Long frame: 313 lines × 227 CCKs.
    let long_frame = (u64::from(PAL_LINES_PER_FRAME) + 1) * u64::from(PAL_CCKS_PER_LINE);
    for _ in 0..long_frame {
        a.tick_cck();
    }
    assert!(!a.lof, "LOF toggles to short frame after long frame");
    assert_eq!(a.vpos, 0);

    // Short frame: 312 × 227.
    let short_frame = u64::from(PAL_LINES_PER_FRAME) * u64::from(PAL_CCKS_PER_LINE);
    for _ in 0..short_frame {
        a.tick_cck();
    }
    assert!(a.lof, "LOF toggles back to long after short");
}

#[test]
fn interlace_disabled_never_produces_a_313_line_frame() {
    let mut a = Agnus::new();
    // BPLCON0 = 0 → LACE off.
    for _ in 0..(u64::from(PAL_LINES_PER_FRAME) * u64::from(PAL_CCKS_PER_LINE)) {
        a.tick_cck();
    }
    assert_eq!(a.vpos, 0, "non-interlace: always 312 lines");
}

// ─── NTSC region ───────────────────────────────────────────────────

#[test]
fn ntsc_region_uses_262_lines() {
    let mut a = Agnus::new_with_region_lines(262);
    // 262 × 227 = 59,474 CCKs.
    for _ in 0..(262 * 227) {
        a.tick_cck();
    }
    assert_eq!(a.vpos, 0);
}
