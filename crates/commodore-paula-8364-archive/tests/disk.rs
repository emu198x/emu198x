//! Phase 1 characterization tests — disk DMA + MFM sync.
//!
//! Per HRM chapter 8 (Disk Controller) and chapter 2 (DSKBYTR layout).
//!
//! The disk controller has four CPU-facing registers:
//!   $020/022  DSKPT   — DMA pointer (chip RAM, 20-bit, word aligned)
//!   $024      DSKLEN  — length + control (DMAEN, WRITE, 14-bit count)
//!   $026      DSKDAT  — raw MFM write data
//!   $07E      DSKSYNC — MFM sync word (standard = $4489)
//!   $01A (r)  DSKBYTR — byte received + status
//!   $01E (r)  INTREQR — DSKSYN + DSKBLK + RBF flags
//!
//! DSKLEN has a two-write arming flip-flop: the DMAEN bit (15) must be
//! turned on twice in a row to actually start DMA. A bit-15=0 write
//! disarms (HRM-recommended $4000 "safety" clear after transfer).
//!
//! DSKBYTR read fields:
//!   bit 15  DSKBYT    — byte-ready flag; cleared on DSKBYTR read
//!   bit 14  DMAON     — DSKLEN.DMAEN & DMACON.DSKEN & DMACON.DMAEN
//!   bit 13  DISKWRITE — DSKLEN.WRITE
//!   bit 12  WORDEQUAL — DSKDATR == DSKSYNC (latched, delayed release)
//!   bits 7-0  DATA
//!
//! MFM byte pacing is ADKCON.FAST-dependent: 14 CCK/byte fast, 28 slow.

use commodore_paula_8364::Paula8364;

const DMACON_MASTER: u16 = 0x0200;
const DMACON_DSK: u16 = 0x0010;
const ADKCON_FAST_DISK: u16 = 0x0100;
const INTREQ_DSKBLK: u16 = 0x0002;

// ────────────────────────────────────────────────────────────────
// DSKLEN double-write arming
// ────────────────────────────────────────────────────────────────

#[test]
fn dsklen_first_write_with_dmaen_only_arms_does_not_start_dma() {
    let mut paula = Paula8364::new();
    paula.write_dsklen(0x8200); // DMAEN + 0x200 words
    assert!(!paula.disk_dma_pending,
        "first bit-15=1 write must only arm — no DMA start yet");
}

#[test]
fn dsklen_second_write_with_dmaen_starts_dma() {
    let mut paula = Paula8364::new();
    paula.write_dsklen(0x8200); // arm
    paula.write_dsklen(0x8200); // trigger
    assert!(paula.disk_dma_pending, "second bit-15=1 write must start DMA");
}

#[test]
fn dsklen_bit_15_zero_disarms_the_flip_flop() {
    // HRM's recommended after-transfer safety sequence is write $4000
    // (WRITE=1, DMAEN=0). That MUST disarm any previous arming, so
    // a subsequent bit-15=1 write only arms, not triggers.
    let mut paula = Paula8364::new();
    paula.write_dsklen(0x8200); // arm
    paula.write_dsklen(0x4000); // $4000 safety write → disarm
    paula.write_dsklen(0x8200); // only arm again
    assert!(!paula.disk_dma_pending,
        "bit-15=0 write disarms; a single later bit-15=1 must not trigger");
    paula.write_dsklen(0x8200); // now trigger
    assert!(paula.disk_dma_pending);
}

#[test]
fn complete_disk_dma_clears_pending_and_raises_dskblk_irq() {
    let mut paula = Paula8364::new();
    paula.write_dsklen(0x8200);
    paula.write_dsklen(0x8200);
    assert!(paula.disk_dma_pending);

    paula.complete_disk_dma();
    assert!(!paula.disk_dma_pending);
    assert_ne!(paula.intreq & INTREQ_DSKBLK, 0,
        "completion must raise INTREQ.DSKBLK (bit 1)");
}

// ────────────────────────────────────────────────────────────────
// DSKBYTR status bits
// ────────────────────────────────────────────────────────────────

#[test]
fn dskbytr_byt_bit_latches_on_word_reception_and_clears_on_read() {
    let mut paula = Paula8364::new();
    paula.note_disk_read_word(0xABCD);
    let first = paula.read_dskbytr(0);
    assert_ne!(first & 0x8000, 0, "DSKBYT latched after word reception");
    let second = paula.read_dskbytr(0);
    assert_eq!(second & 0x8000, 0, "DSKBYT clears on read — HRM");
}

#[test]
fn dskbytr_data_byte_is_the_high_byte_of_the_received_word() {
    let mut paula = Paula8364::new();
    paula.note_disk_read_word(0xA1B2);
    let byt = paula.read_dskbytr(0);
    assert_eq!(byt & 0x00FF, 0xA1, "DATA bits reflect the received word's high byte first");
}

#[test]
fn dskbytr_reports_dmaon_only_when_all_three_enables_are_set() {
    let mut paula = Paula8364::new();
    paula.write_dsklen(0x8200); paula.write_dsklen(0x8200);
    let dmacon_full = DMACON_MASTER | DMACON_DSK;
    let dmacon_partial = DMACON_MASTER; // master, no DSK bit

    assert_ne!(paula.read_dskbytr(dmacon_full) & 0x4000, 0,
        "DMAON = DSKLEN.DMAEN & DMACON.DSKEN & DMACON.DMAEN — all three required");
    assert_eq!(paula.read_dskbytr(dmacon_partial) & 0x4000, 0,
        "DSKEN (DMACON bit 4) missing → DMAON low");
}

#[test]
fn dskbytr_reports_diskwrite_from_dsklen_bit_14() {
    let mut paula = Paula8364::new();
    paula.write_dsklen(0xC200); // DMAEN + WRITE
    paula.write_dsklen(0xC200);
    let byt = paula.read_dskbytr(DMACON_MASTER | DMACON_DSK);
    assert_ne!(byt & 0x2000, 0, "DISKWRITE = DSKLEN bit 14");
}

// ────────────────────────────────────────────────────────────────
// DSKSYNC / WORDEQUAL
// ────────────────────────────────────────────────────────────────

#[test]
fn received_word_matching_dsksync_returns_true_from_note_disk_read_word() {
    let mut paula = Paula8364::new();
    paula.dsksync = 0x4489;
    assert!(paula.note_disk_read_word(0x4489),
        "word matching DSKSYNC reports equality");
    assert!(!paula.note_disk_read_word(0x4488));
}

#[test]
fn wordequal_bit_latches_until_delay_elapses_then_clears() {
    let mut paula = Paula8364::new();
    paula.dsksync = 0x4489;
    assert!(paula.note_disk_read_word(0x4489));

    // Slow mode → 28-CCK delay before WORDEQUAL clears.
    let byt = paula.read_dskbytr(0);
    assert_ne!(byt & 0x1000, 0, "WORDEQUAL latched immediately on match");

    for _ in 0..28 {
        paula.tick_disk_cck();
    }
    let byt = paula.read_dskbytr(0);
    assert_eq!(byt & 0x1000, 0, "WORDEQUAL clears after the sync-match delay");
}

// ────────────────────────────────────────────────────────────────
// Byte pacing (ADKCON.FAST)
// ────────────────────────────────────────────────────────────────

#[test]
fn fast_disk_bit_picks_14_cck_per_byte() {
    let mut paula = Paula8364::new();
    paula.write_adkcon(0x8000 | ADKCON_FAST_DISK);
    paula.note_disk_read_word(0xAABB);
    // Drain the immediate high-byte.
    let _ = paula.read_dskbytr(0);

    // Tick until DSKBYT returns — should be 14 CCK in fast mode.
    let mut elapsed = 0;
    for _ in 1..=40 {
        paula.tick_disk_cck();
        elapsed += 1;
        if paula.read_dskbytr(0) & 0x8000 != 0 {
            break;
        }
    }
    assert_eq!(elapsed, 14);
}

#[test]
fn slow_disk_default_picks_28_cck_per_byte() {
    let mut paula = Paula8364::new();
    paula.note_disk_read_word(0xAABB);
    let _ = paula.read_dskbytr(0);

    let mut elapsed = 0;
    for _ in 1..=60 {
        paula.tick_disk_cck();
        elapsed += 1;
        if paula.read_dskbytr(0) & 0x8000 != 0 {
            break;
        }
    }
    assert_eq!(elapsed, 28, "default (ADKCON.FAST clear) paces at 28 CCK/byte");
}

// ────────────────────────────────────────────────────────────────
// DSKDAT write path (PIO + DMA queue)
// ────────────────────────────────────────────────────────────────

#[test]
fn dskdat_writes_queue_in_program_order_for_the_drive_consumer() {
    let mut paula = Paula8364::new();
    paula.write_dskdat(0x1111);
    paula.write_dskdat(0x2222);
    paula.write_dskdat(0x3333);
    assert_eq!(paula.dskdat_queue_len(), 3);
    assert_eq!(paula.take_dskdat_queued_word(), Some(0x1111));
    assert_eq!(paula.take_dskdat_queued_word(), Some(0x2222));
    assert_eq!(paula.take_dskdat_queued_word(), Some(0x3333));
    assert_eq!(paula.take_dskdat_queued_word(), None);
}

#[test]
fn disk_write_dma_log_and_pio_log_are_separate_channels() {
    let mut paula = Paula8364::new();
    paula.note_disk_write_dma_word(0xAAAA);
    paula.note_disk_write_pio_word(0xBBBB);
    paula.note_disk_write_dma_word(0xCCCC);
    assert_eq!(paula.disk_write_dma_log(), &[0xAAAA, 0xCCCC]);
    assert_eq!(paula.disk_write_pio_log(), &[0xBBBB]);

    paula.clear_disk_write_dma_log();
    assert!(paula.disk_write_dma_log().is_empty());
    assert_eq!(paula.disk_write_pio_log(), &[0xBBBB],
        "clearing one log must not affect the other");
}

// ────────────────────────────────────────────────────────────────
// Disk PLL (variable-rate IPF tracks)
// ────────────────────────────────────────────────────────────────

#[test]
fn disk_pll_accumulates_to_16_bits_before_word_ready() {
    let mut paula = Paula8364::new();
    // Feed 8 half-bit-cells: not yet a full word.
    for _ in 0..8 {
        assert!(!paula.disk_pll_accumulate(1));
    }
    // Next 8 complete the word.
    for _ in 0..7 {
        assert!(!paula.disk_pll_accumulate(1));
    }
    assert!(paula.disk_pll_accumulate(1),
        "16 bit-cells make a word — PLL signals ready");
}

#[test]
fn disk_pll_reset_clears_accumulator() {
    let mut paula = Paula8364::new();
    for _ in 0..15 { paula.disk_pll_accumulate(1); }
    paula.disk_pll_reset();
    assert!(!paula.disk_pll_accumulate(15),
        "after reset, 15 cells alone must not trigger a word");
    assert!(paula.disk_pll_accumulate(1),
        "one more cell reaches 16");
}
