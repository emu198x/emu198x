//! Phase 1 characterization tests — disk DMA + MFM sync.
//!
//! Per HRM chapter 8 (Disk Controller) and chapter 2 (DSKBYTR layout).
//!
//! Key invariants:
//!   - DSKLEN DMAEN write must be doubled to actually start DMA.
//!   - Bit-15=0 DSKLEN write disarms the flip-flop (HRM "$4000 safety").
//!   - DSKBYTR.DSKBYT clears on read; WORDEQUAL latches with a delay.
//!   - Byte pacing is 14 CCK (ADKCON.FAST) or 28 CCK (default).
//!   - The disk PLL consumes 16 bit-cells per word in variable-rate mode.

use commodore_paula_8364::{IntSource, Paula8364, bits::*};

// ────────────────────────────────────────────────────────────────
// DSKLEN double-write arming
// ────────────────────────────────────────────────────────────────

#[test]
fn dsklen_first_write_with_dmaen_only_arms_does_not_start_dma() {
    let mut p = Paula8364::new();
    p.write_dsklen(DSKLEN_DMAEN | 0x0200);
    assert!(
        !p.disk_dma_pending(),
        "first bit-15=1 write must only arm — no DMA start yet"
    );
}

#[test]
fn dsklen_second_write_with_dmaen_starts_dma() {
    let mut p = Paula8364::new();
    p.write_dsklen(DSKLEN_DMAEN | 0x0200);
    p.write_dsklen(DSKLEN_DMAEN | 0x0200);
    assert!(p.disk_dma_pending());
}

#[test]
fn dsklen_bit_15_zero_disarms_the_flip_flop() {
    let mut p = Paula8364::new();
    p.write_dsklen(DSKLEN_DMAEN | 0x0200); // arm
    p.write_dsklen(DSKLEN_WRITE); // $4000 safety → disarm
    p.write_dsklen(DSKLEN_DMAEN | 0x0200); // only arm again
    assert!(!p.disk_dma_pending());
    p.write_dsklen(DSKLEN_DMAEN | 0x0200); // now trigger
    assert!(p.disk_dma_pending());
}

#[test]
fn complete_disk_dma_clears_pending_and_raises_dskblk_irq() {
    let mut p = Paula8364::new();
    p.write_dsklen(DSKLEN_DMAEN | 0x0200);
    p.write_dsklen(DSKLEN_DMAEN | 0x0200);
    assert!(p.disk_dma_pending());

    p.complete_disk_dma();
    assert!(!p.disk_dma_pending());
    assert_ne!(
        p.intreq() & IntSource::DskBlk.mask(),
        0,
        "completion raises INTREQ.DSKBLK"
    );
}

// ────────────────────────────────────────────────────────────────
// DSKBYTR status bits
// ────────────────────────────────────────────────────────────────

#[test]
fn dskbytr_byt_bit_latches_on_word_reception_and_clears_on_read() {
    let mut p = Paula8364::new();
    p.note_disk_read_word(0xABCD);
    let first = p.read_dskbytr(0);
    assert_ne!(first & DSKBYTR_DSKBYT, 0);
    let second = p.read_dskbytr(0);
    assert_eq!(second & DSKBYTR_DSKBYT, 0, "DSKBYT clears on read");
}

#[test]
fn peek_dskbytr_is_side_effect_free() {
    let mut p = Paula8364::new();
    p.note_disk_read_word(0xABCD);
    assert_ne!(p.peek_dskbytr(0) & DSKBYTR_DSKBYT, 0);
    assert_ne!(
        p.peek_dskbytr(0) & DSKBYTR_DSKBYT,
        0,
        "peek must not clear DSKBYT"
    );
    let _ = p.read_dskbytr(0);
    assert_eq!(p.peek_dskbytr(0) & DSKBYTR_DSKBYT, 0);
}

#[test]
fn dskbytr_data_byte_is_the_high_byte_of_the_received_word() {
    let mut p = Paula8364::new();
    p.note_disk_read_word(0xA1B2);
    let byt = p.read_dskbytr(0);
    assert_eq!(byt & DSKBYTR_DATA_MASK, 0xA1);
}

#[test]
fn dskbytr_reports_dmaon_only_when_all_three_enables_are_set() {
    let mut p = Paula8364::new();
    p.write_dsklen(DSKLEN_DMAEN | 0x0200);
    p.write_dsklen(DSKLEN_DMAEN | 0x0200);
    let full = DMA_MASTER | DMA_DSK;
    let partial = DMA_MASTER;
    assert_ne!(p.read_dskbytr(full) & DSKBYTR_DMAON, 0);
    assert_eq!(p.read_dskbytr(partial) & DSKBYTR_DMAON, 0);
}

#[test]
fn dskbytr_reports_diskwrite_from_dsklen_bit_14() {
    let mut p = Paula8364::new();
    p.write_dsklen(DSKLEN_DMAEN | DSKLEN_WRITE | 0x0200);
    p.write_dsklen(DSKLEN_DMAEN | DSKLEN_WRITE | 0x0200);
    let byt = p.read_dskbytr(DMA_MASTER | DMA_DSK);
    assert_ne!(byt & DSKBYTR_DISKWRITE, 0);
}

// ────────────────────────────────────────────────────────────────
// DSKSYNC / WORDEQUAL
// ────────────────────────────────────────────────────────────────

#[test]
fn received_word_matching_dsksync_returns_true() {
    let mut p = Paula8364::new();
    p.set_dsksync(0x4489);
    assert!(p.note_disk_read_word(0x4489));
    assert!(!p.note_disk_read_word(0x4488));
}

#[test]
fn sync_match_raises_int_dsksyn_only_when_adkcon_wordsync_is_set() {
    // HRM: WORDSYNC controls whether the comparator's match raises
    // INT_DSKSYN. DSKBYTR.WORDEQUAL latches independently.
    let mut p = Paula8364::new();
    p.set_dsksync(0x4489);

    // Match with WORDSYNC clear → no IRQ.
    assert!(p.note_disk_read_word(0x4489));
    assert_eq!(
        p.intreq() & IntSource::DskSyn.mask(),
        0,
        "match with WORDSYNC clear must not raise INT_DSKSYN"
    );

    // Enable WORDSYNC, match again → IRQ.
    p.write_adkcon(INT_SETCLR | ADKCON_WORDSYNC);
    assert!(p.note_disk_read_word(0x4489));
    assert_ne!(
        p.intreq() & IntSource::DskSyn.mask(),
        0,
        "match with WORDSYNC set raises INT_DSKSYN"
    );

    // Non-match with WORDSYNC set → no new IRQ bit (but existing
    // pending bit stays — INTREQ is sticky until cleared).
    p.write_intreq(IntSource::DskSyn.mask()); // CLEAR
    assert!(!p.note_disk_read_word(0xAAAA));
    assert_eq!(p.intreq() & IntSource::DskSyn.mask(), 0);
}

#[test]
fn wordequal_bit_latches_until_delay_elapses_then_clears() {
    let mut p = Paula8364::new();
    p.set_dsksync(0x4489);
    assert!(p.note_disk_read_word(0x4489));

    let byt = p.read_dskbytr(0);
    assert_ne!(byt & DSKBYTR_WORDEQUAL, 0);

    for _ in 0..DISK_BYTE_CCK_SLOW {
        p.tick_disk_cck();
    }
    let byt = p.read_dskbytr(0);
    assert_eq!(
        byt & DSKBYTR_WORDEQUAL,
        0,
        "WORDEQUAL clears after the sync-match delay"
    );
}

// ────────────────────────────────────────────────────────────────
// Byte pacing (ADKCON.FAST)
// ────────────────────────────────────────────────────────────────

#[test]
fn fast_disk_bit_picks_14_cck_per_byte() {
    let mut p = Paula8364::new();
    p.write_adkcon(INT_SETCLR | ADKCON_FAST);
    p.note_disk_read_word(0xAABB);
    let _ = p.read_dskbytr(0);

    let mut elapsed = 0u8;
    for _ in 1..=40u8 {
        p.tick_disk_cck();
        elapsed += 1;
        if p.read_dskbytr(0) & DSKBYTR_DSKBYT != 0 {
            break;
        }
    }
    assert_eq!(elapsed, DISK_BYTE_CCK_FAST);
}

#[test]
fn slow_disk_default_picks_28_cck_per_byte() {
    let mut p = Paula8364::new();
    p.note_disk_read_word(0xAABB);
    let _ = p.read_dskbytr(0);

    let mut elapsed = 0u8;
    for _ in 1..=60u8 {
        p.tick_disk_cck();
        elapsed += 1;
        if p.read_dskbytr(0) & DSKBYTR_DSKBYT != 0 {
            break;
        }
    }
    assert_eq!(elapsed, DISK_BYTE_CCK_SLOW);
}

// ────────────────────────────────────────────────────────────────
// DSKDAT write path (PIO + DMA queue)
// ────────────────────────────────────────────────────────────────

#[test]
fn dskdat_writes_queue_in_program_order_for_the_drive_consumer() {
    let mut p = Paula8364::new();
    p.write_dskdat(0x1111);
    p.write_dskdat(0x2222);
    p.write_dskdat(0x3333);
    assert_eq!(p.dskdat_queue_len(), 3);
    assert_eq!(p.take_dskdat_queued_word(), Some(0x1111));
    assert_eq!(p.take_dskdat_queued_word(), Some(0x2222));
    assert_eq!(p.take_dskdat_queued_word(), Some(0x3333));
    assert_eq!(p.take_dskdat_queued_word(), None);
}

#[test]
fn disk_write_dma_log_and_pio_log_are_separate_channels() {
    let mut p = Paula8364::new();
    p.note_disk_write_dma_word(0xAAAA);
    p.note_disk_write_pio_word(0xBBBB);
    p.note_disk_write_dma_word(0xCCCC);
    assert_eq!(p.debug_disk_write_dma_log(), &[0xAAAA, 0xCCCC]);
    assert_eq!(p.debug_disk_write_pio_log(), &[0xBBBB]);

    p.clear_debug_disk_write_dma_log();
    assert!(p.debug_disk_write_dma_log().is_empty());
    assert_eq!(p.debug_disk_write_pio_log(), &[0xBBBB]);
}

// ────────────────────────────────────────────────────────────────
// Disk PLL (variable-rate IPF tracks)
// ────────────────────────────────────────────────────────────────

#[test]
fn disk_pll_accumulates_to_16_bits_before_word_ready() {
    let mut p = Paula8364::new();
    for _ in 0..15 {
        assert!(!p.disk_pll_accumulate(1));
    }
    assert!(
        p.disk_pll_accumulate(1),
        "16 bit-cells make a word — PLL signals ready"
    );
}

#[test]
fn disk_pll_reset_clears_accumulator() {
    let mut p = Paula8364::new();
    for _ in 0..15 {
        p.disk_pll_accumulate(1);
    }
    p.disk_pll_reset();
    assert!(!p.disk_pll_accumulate(15));
    assert!(p.disk_pll_accumulate(1));
}

#[test]
fn disk_pll_variable_rate_toggle_roundtrips() {
    let mut p = Paula8364::new();
    assert!(!p.disk_pll_variable_rate());
    p.set_disk_pll_variable_rate(true);
    assert!(p.disk_pll_variable_rate());
    p.set_disk_pll_variable_rate(false);
    assert!(!p.disk_pll_variable_rate());
}

#[test]
fn disk_diagnostic_snapshot_exposes_register_latch_queue_dma_and_pll_state() {
    let mut p = Paula8364::new();
    p.write_adkcon(INT_SETCLR | ADKCON_WORDSYNC | ADKCON_FAST);
    p.set_dsksync(0x4489);
    p.write_dskdat(0x1111);
    p.write_dskdat(0x2222);
    assert!(p.note_disk_read_word(0x4489));
    p.write_dsklen(DSKLEN_DMAEN | 3);
    p.write_dsklen(DSKLEN_DMAEN | 3);
    assert!(!p.disk_pll_accumulate(5));
    p.set_disk_pll_variable_rate(true);

    let snapshot = p.disk_diagnostic_snapshot();
    assert_eq!(snapshot.dsklen, DSKLEN_DMAEN | 3);
    assert_eq!(snapshot.dsksync, 0x4489);
    assert_eq!(snapshot.dskdatr, 0x4489);
    assert_eq!(snapshot.dskdat, 0x2222);
    assert_eq!(snapshot.dskbytr_data, 0x44);
    assert_eq!(snapshot.dskbytr_next_data, Some(0x89));
    assert_eq!(snapshot.dskbytr_next_delay_cck, DISK_BYTE_CCK_FAST);
    assert!(snapshot.dskbytr_valid);
    assert!(snapshot.dskbytr_wordequal);
    assert_eq!(snapshot.dskbytr_wordequal_delay_cck, DISK_BYTE_CCK_FAST);
    assert_eq!(snapshot.dskdat_queue, [0x1111, 0x2222]);
    assert!(!snapshot.dsklen_armed);
    assert!(snapshot.disk_dma_pending);
    assert_eq!(snapshot.disk_dma_words_remaining, 3);
    assert!(!snapshot.disk_dma_is_write);
    assert!(snapshot.disk_dma_wordsync_waiting);
    assert!(!snapshot.disk_dma_write_active);
    assert!(snapshot.dsklen_dma_enabled);
    assert!(!snapshot.dsklen_write_enabled);
    assert!(snapshot.wordsync_enabled);
    assert!(snapshot.fast_enabled);
    assert_eq!(snapshot.disk_byte_delay_cck, DISK_BYTE_CCK_FAST);
    assert_eq!(snapshot.disk_pll_phase, 5);
    assert!(snapshot.disk_pll_variable_rate);

    assert_eq!(p.dskdat_queue_len(), 2, "snapshot must not drain DSKDAT");
    assert_eq!(p.take_dskdat_queued_word(), Some(0x1111));
}

#[test]
fn disk_diagnostic_snapshot_exposes_arming_and_write_direction() {
    let mut p = Paula8364::new();
    let dsklen = DSKLEN_DMAEN | DSKLEN_WRITE | 2;

    p.write_dsklen(dsklen);
    let armed = p.disk_diagnostic_snapshot();
    assert!(armed.dsklen_armed);
    assert!(!armed.disk_dma_pending);
    assert_eq!(armed.disk_dma_words_remaining, 0);
    assert!(armed.dsklen_write_enabled);

    p.write_dsklen(dsklen);
    let active = p.disk_diagnostic_snapshot();
    assert!(!active.dsklen_armed);
    assert!(active.disk_dma_pending);
    assert_eq!(active.disk_dma_words_remaining, 2);
    assert!(active.disk_dma_is_write);
    assert!(!active.disk_dma_wordsync_waiting);
    assert!(active.disk_dma_write_active);
}
