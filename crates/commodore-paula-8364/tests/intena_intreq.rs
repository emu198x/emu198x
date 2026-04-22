//! Phase 1 characterization tests — INTENA / INTREQ + IPL.
//!
//! Per HRM pp.32-34:
//!
//!  - $09A INTENA — write-only. Bit 15 = SET (OR the low 15 bits into
//!    the mask); bit 15 = 0 means CLEAR. Bit 14 is the master enable
//!    (INTEN); with bit 14 clear in INTENA, no interrupts reach the
//!    CPU regardless of INTREQ.
//!  - $09C INTREQ — same SET/CLEAR semantics.
//!  - $01C INTENAR / $01E INTREQR — read-only mirrors (handled by the
//!    machine-crate custom-register bus).
//!  - 14 sources map to 6 CPU interrupt levels, highest wins.

use commodore_paula_8364::{IntSource, Paula8364, bits::*};

// ────────────────────────────────────────────────────────────────
// SET/CLEAR semantics
// ────────────────────────────────────────────────────────────────

#[test]
fn intena_set_or_clear_selects_on_bit_15() {
    let mut p = Paula8364::new();
    p.write_intena(INT_SETCLR | INT_VERTB);
    assert_eq!(p.intena(), INT_VERTB);
    p.write_intena(INT_SETCLR | INT_AUD0); // must not clear VERTB
    assert_eq!(p.intena(), INT_VERTB | INT_AUD0);
    p.write_intena(INT_VERTB); // CLEAR only VERTB
    assert_eq!(p.intena(), INT_AUD0);
}

#[test]
fn intreq_set_or_clear_selects_on_bit_15() {
    let mut p = Paula8364::new();
    p.write_intreq(INT_SETCLR | INT_VERTB);
    assert_eq!(p.intreq(), INT_VERTB);
    p.write_intreq(INT_VERTB);
    assert_eq!(p.intreq(), 0);
}

#[test]
fn intena_clear_of_untouched_bits_is_a_noop() {
    let mut p = Paula8364::new();
    p.write_intena(INT_SETCLR | 0x00FF); // set the low eight sources
    assert_eq!(p.intena() & 0x00FF, 0x00FF);
    p.write_intena(0x0000); // bit 15 = 0, nothing to clear
    assert_eq!(p.intena() & 0x00FF, 0x00FF);
}

#[test]
fn intena_bit_15_is_not_part_of_the_mask() {
    // The SET flag is a write-only command — it never reads back.
    let mut p = Paula8364::new();
    p.write_intena(0xFFFF);
    assert_eq!(p.intena() & INT_SETCLR, 0);
    assert_ne!(p.intena() & INT_INTEN, 0, "INT_INTEN (bit 14) IS latched");
}

// ────────────────────────────────────────────────────────────────
// raise / raise_intreq helpers
// ────────────────────────────────────────────────────────────────

#[test]
fn raise_by_source_sets_the_matching_bit() {
    let mut p = Paula8364::new();
    p.raise(IntSource::Vertb);
    assert_eq!(p.intreq(), INT_VERTB);
    p.raise(IntSource::Exter);
    assert_eq!(p.intreq(), INT_VERTB | INT_EXTER);
    p.raise(IntSource::Vertb); // idempotent
    assert_eq!(p.intreq(), INT_VERTB | INT_EXTER);
}

#[test]
fn raise_intreq_mask_sets_multiple_bits_in_one_call() {
    let mut p = Paula8364::new();
    p.raise_intreq(INT_COPER | INT_BLIT);
    assert_eq!(p.intreq(), INT_COPER | INT_BLIT);
}

// ────────────────────────────────────────────────────────────────
// Master enable gate (bit 14) — `compute_ipl`
// ────────────────────────────────────────────────────────────────

#[test]
fn compute_ipl_returns_zero_when_master_enable_is_clear() {
    let mut p = Paula8364::new();
    p.write_intena(INT_SETCLR | INT_VERTB); // VERTB enabled, master NOT
    p.raise(IntSource::Vertb);
    assert_eq!(p.compute_ipl(), 0);

    p.write_intena(INT_SETCLR | INT_INTEN);
    assert_eq!(p.compute_ipl(), 3);
}

#[test]
fn compute_ipl_returns_zero_with_no_pending_unmasked_source() {
    let mut p = Paula8364::new();
    p.write_intena(INT_SETCLR | INT_INTEN | INT_SOURCES); // master + every source
    assert_eq!(p.compute_ipl(), 0, "nothing pending");

    p.raise(IntSource::Vertb);
    // Now mask VERTB off again: master only.
    p.write_intena(INT_SOURCES); // clear every source (keep INTEN set)
    assert_eq!(p.compute_ipl(), 0, "VERTB pending but masked");
}

// ────────────────────────────────────────────────────────────────
// IPL priority encoding (HRM Table 3-3)
// ────────────────────────────────────────────────────────────────

fn arm_and_fire(p: &mut Paula8364, src: IntSource) {
    p.write_intena(INT_SETCLR | INT_INTEN | src.mask());
    p.raise(src);
}

#[test]
fn ipl_level_1_for_tbe_dskblk_soft() {
    for src in [IntSource::Tbe, IntSource::DskBlk, IntSource::Soft] {
        let mut p = Paula8364::new();
        arm_and_fire(&mut p, src);
        assert_eq!(p.compute_ipl(), 1, "{:?} → L1", src);
    }
}

#[test]
fn ipl_level_2_for_ports() {
    let mut p = Paula8364::new();
    arm_and_fire(&mut p, IntSource::Ports);
    assert_eq!(p.compute_ipl(), 2);
}

#[test]
fn ipl_level_3_for_coper_vertb_blit() {
    for src in [IntSource::Coper, IntSource::Vertb, IntSource::Blit] {
        let mut p = Paula8364::new();
        arm_and_fire(&mut p, src);
        assert_eq!(p.compute_ipl(), 3);
    }
}

#[test]
fn ipl_level_4_for_audio_channels() {
    for src in [
        IntSource::Aud0,
        IntSource::Aud1,
        IntSource::Aud2,
        IntSource::Aud3,
    ] {
        let mut p = Paula8364::new();
        arm_and_fire(&mut p, src);
        assert_eq!(p.compute_ipl(), 4);
    }
}

#[test]
fn ipl_level_5_for_rbf_and_dsksyn() {
    for src in [IntSource::Rbf, IntSource::DskSyn] {
        let mut p = Paula8364::new();
        arm_and_fire(&mut p, src);
        assert_eq!(p.compute_ipl(), 5);
    }
}

#[test]
fn ipl_level_6_for_exter() {
    let mut p = Paula8364::new();
    arm_and_fire(&mut p, IntSource::Exter);
    assert_eq!(p.compute_ipl(), 6);
}

#[test]
fn ipl_picks_the_highest_unmasked_pending_level() {
    let mut p = Paula8364::new();
    p.write_intena(INT_SETCLR | INT_INTEN | INT_SOURCES);
    p.raise_intreq(INT_SOURCES);
    assert_eq!(p.compute_ipl(), 6);

    p.write_intreq(INT_EXTER); // CLEAR EXTER
    assert_eq!(p.compute_ipl(), 5);

    p.write_intreq(INT_RBF | INT_DSKSYN);
    assert_eq!(p.compute_ipl(), 4);

    p.write_intreq(INT_AUD0 | INT_AUD1 | INT_AUD2 | INT_AUD3);
    assert_eq!(p.compute_ipl(), 3);

    p.write_intreq(INT_COPER | INT_VERTB | INT_BLIT);
    assert_eq!(p.compute_ipl(), 2);

    p.write_intreq(INT_PORTS);
    assert_eq!(p.compute_ipl(), 1);
}

#[test]
fn ipl_returns_zero_when_pending_bits_are_all_masked_off() {
    let mut p = Paula8364::new();
    p.write_intena(INT_SETCLR | INT_INTEN | INT_VERTB);
    p.raise(IntSource::Exter);
    assert_eq!(p.compute_ipl(), 0);
}

// ────────────────────────────────────────────────────────────────
// Write logs
// ────────────────────────────────────────────────────────────────

#[test]
fn intena_write_log_captures_every_write_and_rings_at_16() {
    let mut p = Paula8364::new();
    for i in 0..20u16 {
        p.write_intena(INT_SETCLR | i);
    }
    let log = p.debug_intena_writes();
    assert_eq!(log.len(), 16);
    assert_eq!(*log.front().unwrap(), INT_SETCLR | 4);
    assert_eq!(*log.back().unwrap(), INT_SETCLR | 19);
}

#[test]
fn intreq_write_log_is_independent_from_intena_log() {
    let mut p = Paula8364::new();
    p.write_intena(INT_SETCLR | INT_VERTB);
    p.write_intreq(INT_SETCLR | INT_TBE);
    assert_eq!(p.debug_intena_writes().len(), 1);
    assert_eq!(p.debug_intreq_writes().len(), 1);
}

// ────────────────────────────────────────────────────────────────
// Reset
// ────────────────────────────────────────────────────────────────

#[test]
fn reset_clears_intena_and_intreq_and_logs() {
    let mut p = Paula8364::new();
    p.write_intena(INT_SETCLR | 0x7FFF);
    p.write_intreq(INT_SETCLR | 0x7FFF);
    p.reset();
    assert_eq!(p.intena(), 0);
    assert_eq!(p.intreq(), 0);
    assert!(p.debug_intena_writes().is_empty());
    assert!(p.debug_intreq_writes().is_empty());
}
