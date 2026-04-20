//! Phase 1 characterization tests — INTENA / INTREQ + IPL.
//!
//! Per HRM *Hardware Reference Manual* pp.32-34:
//!
//!  - $09A INTENA — write-only. Bit 15 = SET (OR the low 15 bits into
//!    the mask); bit 15 = 0 means CLEAR. Bit 14 is the master enable
//!    (INTEN); if bit 14 in INTENA is clear, no interrupts reach the
//!    CPU regardless of INTREQ.
//!  - $09C INTREQ — same SET/CLEAR semantics for the request register.
//!  - $01C INTENAR / $01E INTREQR — read-only mirrors.
//!  - 14 sources (bits 0-13) map onto 6 CPU interrupt levels:
//!      L1: TBE(0), DSKBLK(1), SOFT(2)
//!      L2: PORTS(3)
//!      L3: COPER(4), VERTB(5), BLIT(6)
//!      L4: AUD0(7), AUD1(8), AUD2(9), AUD3(10)
//!      L5: RBF(11), DSKSYN(12)
//!      L6: EXTER(13)
//!  - `compute_ipl()` must return the highest pending level, or 0 if
//!    INTEN (bit 14) is clear or no pending source is unmasked.

use commodore_paula_8364::Paula8364;

// ────────────────────────────────────────────────────────────────
// SET/CLEAR semantics
// ────────────────────────────────────────────────────────────────

#[test]
fn intena_set_or_clear_selects_on_bit_15() {
    let mut paula = Paula8364::new();
    paula.write_intena(0x8020); // SET VERTB
    assert_eq!(paula.intena, 0x0020);
    paula.write_intena(0x8080); // SET AUD0 — must not clear VERTB
    assert_eq!(paula.intena, 0x00A0);
    paula.write_intena(0x0020); // CLEAR VERTB only
    assert_eq!(paula.intena, 0x0080);
}

#[test]
fn intreq_set_or_clear_selects_on_bit_15() {
    let mut paula = Paula8364::new();
    paula.write_intreq(0x8020);
    assert_eq!(paula.intreq, 0x0020);
    paula.write_intreq(0x0020); // clear
    assert_eq!(paula.intreq, 0x0000);
}

#[test]
fn intena_clear_of_untouched_bits_is_a_noop() {
    let mut paula = Paula8364::new();
    paula.write_intena(0x80FF); // set lower 8 sources + soft
    assert_eq!(paula.intena, 0x00FF);
    paula.write_intena(0x0000); // bit 15 = 0, low 15 = 0 → nothing to clear
    assert_eq!(paula.intena, 0x00FF, "clear with 0 mask touches nothing");
}

#[test]
fn intena_bit_15_is_not_part_of_the_mask() {
    // The SET flag lives in bit 15 but is not itself a stored bit —
    // INTENA reads back with bit 15 = 0.
    let mut paula = Paula8364::new();
    paula.write_intena(0xFFFF); // SET all 15 sources + bit 14 (INTEN)
    assert_eq!(paula.intena & 0x8000, 0, "bit 15 is not latched into INTENA");
    assert_ne!(paula.intena & 0x4000, 0, "bit 14 (INTEN) IS latched");
}

// ────────────────────────────────────────────────────────────────
// request_interrupt helper
// ────────────────────────────────────────────────────────────────

#[test]
fn request_interrupt_sets_the_corresponding_intreq_bit() {
    let mut paula = Paula8364::new();
    paula.request_interrupt(5); // VERTB
    assert_eq!(paula.intreq, 0x0020);
    paula.request_interrupt(13); // EXTER
    assert_eq!(paula.intreq, 0x2020);
    paula.request_interrupt(5); // re-request — already set, idempotent
    assert_eq!(paula.intreq, 0x2020);
}

// ────────────────────────────────────────────────────────────────
// Master enable gate (bit 14)
// ────────────────────────────────────────────────────────────────

#[test]
fn compute_ipl_returns_zero_when_master_enable_is_clear() {
    let mut paula = Paula8364::new();
    paula.intena = 0x0020; // VERTB enabled but INTEN (bit 14) clear
    paula.intreq = 0x0020;
    assert_eq!(paula.compute_ipl(), 0);
    paula.intena = 0x4020; // set master-enable
    assert_eq!(paula.compute_ipl(), 3);
}

#[test]
fn compute_ipl_returns_zero_with_no_pending_unmasked_source() {
    let mut paula = Paula8364::new();
    paula.intena = 0x7FFF; // everything enabled
    paula.intreq = 0x0000; // nothing pending
    assert_eq!(paula.compute_ipl(), 0);

    paula.intreq = 0x0020; // VERTB pending, but let's mask it off
    paula.intena = 0x4000; // master only, no VERTB
    assert_eq!(paula.compute_ipl(), 0);
}

// ────────────────────────────────────────────────────────────────
// IPL priority encoding (HRM table)
// ────────────────────────────────────────────────────────────────

#[test]
fn ipl_level_1_for_tbe_dskblk_soft() {
    for bit in [0u8, 1, 2] {
        let mut paula = Paula8364::new();
        paula.intena = 0x4000 | (1 << bit);
        paula.intreq = 1 << bit;
        assert_eq!(paula.compute_ipl(), 1, "bit {bit} → L1");
    }
}

#[test]
fn ipl_level_2_for_ports() {
    let mut paula = Paula8364::new();
    paula.intena = 0x4008;
    paula.intreq = 0x0008;
    assert_eq!(paula.compute_ipl(), 2);
}

#[test]
fn ipl_level_3_for_coper_vertb_blit() {
    for bit in [4u8, 5, 6] {
        let mut paula = Paula8364::new();
        paula.intena = 0x4000 | (1 << bit);
        paula.intreq = 1 << bit;
        assert_eq!(paula.compute_ipl(), 3, "bit {bit} → L3");
    }
}

#[test]
fn ipl_level_4_for_audio_channels() {
    for bit in [7u8, 8, 9, 10] {
        let mut paula = Paula8364::new();
        paula.intena = 0x4000 | (1 << bit);
        paula.intreq = 1 << bit;
        assert_eq!(paula.compute_ipl(), 4, "bit {bit} → L4");
    }
}

#[test]
fn ipl_level_5_for_rbf_and_dsksyn() {
    for bit in [11u8, 12] {
        let mut paula = Paula8364::new();
        paula.intena = 0x4000 | (1 << bit);
        paula.intreq = 1 << bit;
        assert_eq!(paula.compute_ipl(), 5, "bit {bit} → L5");
    }
}

#[test]
fn ipl_level_6_for_exter() {
    let mut paula = Paula8364::new();
    paula.intena = 0x6000;
    paula.intreq = 0x2000;
    assert_eq!(paula.compute_ipl(), 6);
}

#[test]
fn ipl_picks_the_highest_unmasked_pending_level() {
    // All 14 sources pending + enabled → should return L6 (EXTER).
    let mut paula = Paula8364::new();
    paula.intena = 0x7FFF;
    paula.intreq = 0x3FFF;
    assert_eq!(paula.compute_ipl(), 6);

    // Clear EXTER; should fall back to L5.
    paula.intreq = 0x1FFF;
    assert_eq!(paula.compute_ipl(), 5);

    // Clear L5 sources; should fall back to L4.
    paula.intreq = 0x07FF;
    assert_eq!(paula.compute_ipl(), 4);

    // Drop to L3.
    paula.intreq = 0x007F;
    assert_eq!(paula.compute_ipl(), 3);

    // Drop to L2.
    paula.intreq = 0x000F;
    assert_eq!(paula.compute_ipl(), 2);

    // Drop to L1.
    paula.intreq = 0x0007;
    assert_eq!(paula.compute_ipl(), 1);
}

#[test]
fn ipl_returns_zero_when_pending_bits_are_all_masked_off() {
    let mut paula = Paula8364::new();
    // EXTER pending but NOT in INTENA.
    paula.intena = 0x4020; // master + VERTB only
    paula.intreq = 0x2000; // EXTER pending
    assert_eq!(paula.compute_ipl(), 0);
}

// ────────────────────────────────────────────────────────────────
// Write logs
// ────────────────────────────────────────────────────────────────

#[test]
fn intena_write_log_captures_every_write_and_rings_at_16() {
    let mut paula = Paula8364::new();
    for i in 0..20u16 {
        paula.write_intena(0x8000 | i);
    }
    assert_eq!(paula.intena_write_log.len(), 16);
    // Oldest entry should be the write with value $8004 (index 4).
    assert_eq!(*paula.intena_write_log.front().unwrap(), 0x8004);
    assert_eq!(*paula.intena_write_log.back().unwrap(), 0x8013);
}

#[test]
fn intreq_write_log_is_independent_from_intena_log() {
    let mut paula = Paula8364::new();
    paula.write_intena(0x8020);
    paula.write_intreq(0x8001);
    assert_eq!(paula.intena_write_log.len(), 1);
    assert_eq!(paula.intreq_write_log.len(), 1);
    assert_eq!(*paula.intena_write_log.front().unwrap(), 0x8020);
    assert_eq!(*paula.intreq_write_log.front().unwrap(), 0x8001);
}

// ────────────────────────────────────────────────────────────────
// Reset
// ────────────────────────────────────────────────────────────────

#[test]
fn reset_clears_intena_and_intreq_and_logs() {
    let mut paula = Paula8364::new();
    paula.write_intena(0xFFFF);
    paula.write_intreq(0xFFFF);
    paula.reset();
    assert_eq!(paula.intena, 0);
    assert_eq!(paula.intreq, 0);
    assert!(paula.intena_write_log.is_empty());
    assert!(paula.intreq_write_log.is_empty());
}
