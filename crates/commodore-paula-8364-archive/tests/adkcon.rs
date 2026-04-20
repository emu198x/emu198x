//! Phase 1 characterization tests — ADKCON ($09E write / $010 read).
//!
//! Per HRM Audio and Disk chapters. ADKCON is the shared peripheral-
//! control register for Paula's audio and disk subsystems. Bit layout:
//!
//!   bit 15  SET/CLEAR flag (write-only)
//!   bit 14  (unused on OCS)
//!   bit 13  PRECOMP1 (disk write precompensation, upper bit)
//!   bit 12  PRECOMP0 (disk write precompensation, lower bit)
//!   bit 11  MFMPREC  (MFM vs GCR; always 1 for Amiga floppies)
//!   bit 10  UARTBRK  (send serial break)
//!   bit  9  WORDSYNC (latch DSKSYNC compare to read-MFM sync)
//!   bit  8  MSBSYNC  (sync on MSB — unused on Amiga)
//!   bit  8  FAST     (fast disk — 14 CCK/byte vs 28 CCK/byte)
//!   bits 7-4  USE3P3..USE0P1  (audio channel N uses channel N-1 period)
//!   bits 3-0  USE3V2..USE0V1  (audio channel N uses channel N-1 volume)
//!
//! FAST and MSBSYNC share bit 8 per HRM — FAST is the meaningful name
//! in the disk context. The archive uses 0x0100 as FAST_DISK.

use commodore_paula_8364::Paula8364;

// ────────────────────────────────────────────────────────────────
// SET/CLEAR
// ────────────────────────────────────────────────────────────────

#[test]
fn adkcon_write_bit_15_set_adds_to_mask() {
    let mut paula = Paula8364::new();
    paula.write_adkcon(0x8100); // SET FAST (bit 8)
    assert_eq!(paula.adkcon & 0x0100, 0x0100);

    paula.write_adkcon(0x8001); // SET USE0V1 (bit 0) — must not clear FAST
    assert_eq!(paula.adkcon & 0x0101, 0x0101);
}

#[test]
fn adkcon_write_bit_15_clear_removes_from_mask() {
    let mut paula = Paula8364::new();
    paula.write_adkcon(0x8FFF); // SET all bits 0..14
    assert_eq!(paula.adkcon & 0x7FFF, 0x0FFF);

    paula.write_adkcon(0x00FF); // CLEAR low 8 bits
    assert_eq!(paula.adkcon & 0x7FFF, 0x0F00);
}

#[test]
fn adkcon_bit_15_is_not_stored() {
    let mut paula = Paula8364::new();
    paula.write_adkcon(0xFFFF);
    assert_eq!(paula.adkcon & 0x8000, 0, "SET flag lives only in the write");
}

// ────────────────────────────────────────────────────────────────
// FAST disk bit 8 — governs DSKBYTR byte pacing
// ────────────────────────────────────────────────────────────────

#[test]
fn fast_disk_bit_shortens_next_byte_arrival_cck() {
    // Arrange: clock a word through the disk receiver in slow mode,
    // then in fast mode, and observe DSKBYTR.DSKBYT (bit 15) going
    // from 0 → 1 at different CCK counts.
    fn ccks_until_byte_valid(fast: bool) -> u32 {
        let mut paula = Paula8364::new();
        if fast {
            paula.write_adkcon(0x8100);
        }
        // Write a sync word to DSKSYNC so note_disk_read_word has a
        // well-defined WORDEQUAL setup; we only care about byte timing
        // here.
        paula.dsksync = 0x4489;
        let wordequal = paula.note_disk_read_word(0xA1A1);
        assert!(!wordequal);

        // DSKBYT (bit 15 of DSKBYTR) becomes valid *after* the
        // configured delay. Read DSKBYTR once to drain the high byte
        // that `note_disk_read_word` made available immediately.
        let _ = paula.read_dskbytr(0);

        for n in 1..=64u32 {
            paula.tick_disk_cck();
            if paula.read_dskbytr(0) & 0x8000 != 0 {
                return n;
            }
        }
        u32::MAX
    }

    assert_eq!(ccks_until_byte_valid(true), 14, "ADKCON.FAST → 14 CCK/byte");
    assert_eq!(ccks_until_byte_valid(false), 28, "ADKCON slow → 28 CCK/byte");
}

// ────────────────────────────────────────────────────────────────
// Audio attach — ADKCON bits 0-3 (USE_VOLUME) / 4-7 (USE_PERIOD)
// ────────────────────────────────────────────────────────────────

#[test]
fn adkcon_use_period_bits_stored_round_trip() {
    let mut paula = Paula8364::new();
    paula.write_adkcon(0x80F0); // SET all four USE_PERIOD bits (4..7)
    assert_eq!(paula.adkcon & 0x00F0, 0x00F0);
    paula.write_adkcon(0x0040); // CLEAR only USE1P2 (ch 1 modulates ch 2)
    assert_eq!(paula.adkcon & 0x00F0, 0x00B0);
}

#[test]
fn adkcon_use_volume_bits_stored_round_trip() {
    let mut paula = Paula8364::new();
    paula.write_adkcon(0x800F); // SET all four USE_VOLUME bits (0..3)
    assert_eq!(paula.adkcon & 0x000F, 0x000F);
    paula.write_adkcon(0x0001); // CLEAR only USE0V1
    assert_eq!(paula.adkcon & 0x000F, 0x000E);
}

#[test]
fn adkcon_use_period_and_volume_are_independent() {
    let mut paula = Paula8364::new();
    paula.write_adkcon(0x80F0); // SET period bits
    paula.write_adkcon(0x000F); // CLEAR volume bits (already clear; noop)
    assert_eq!(paula.adkcon, 0x00F0);
    paula.write_adkcon(0x8001); // SET USE0V1
    assert_eq!(paula.adkcon, 0x00F1);
}

// ────────────────────────────────────────────────────────────────
// Reset
// ────────────────────────────────────────────────────────────────

#[test]
fn reset_clears_adkcon() {
    let mut paula = Paula8364::new();
    paula.write_adkcon(0xFFFF);
    paula.reset();
    assert_eq!(paula.adkcon, 0);
}
