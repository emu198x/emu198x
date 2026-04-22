//! Phase 1 characterization tests — ADKCON ($09E write / $010 read).
//!
//! Per HRM Audio and Disk chapters. ADKCON is the shared peripheral-
//! control register for Paula's audio and disk subsystems.
//!
//!   bit 15  SET/CLEAR flag (write-only)
//!   bit 14  PRECOMP1 (disk write precompensation, upper bit)
//!   bit 13  PRECOMP0 (disk write precompensation, lower bit)
//!   bit 12  MFMPREC  (MFM vs GCR; always 1 on Amiga floppies)
//!   bit 11  UARTBRK  (send serial break)
//!   bit 10  WORDSYNC (latch DSKSYNC compare)
//!   bit  9  MSBSYNC  (Apple-GCR MSB sync mode)
//!   bit  8  FAST     (disk byte-pacing: 14 vs 28 CCK)
//!   bits 7-4  USE3P3..USE0P1  (N modulates N+1 period)
//!   bits 3-0  USE3V2..USE0V1  (N modulates N+1 volume)

use commodore_paula_8364::{Paula8364, bits::*};

// ────────────────────────────────────────────────────────────────
// SET/CLEAR
// ────────────────────────────────────────────────────────────────

#[test]
fn adkcon_write_bit_15_set_adds_to_mask() {
    let mut p = Paula8364::new();
    p.write_adkcon(INT_SETCLR | ADKCON_FAST);
    assert_eq!(p.adkcon() & ADKCON_FAST, ADKCON_FAST);

    p.write_adkcon(INT_SETCLR | ADKCON_USE_VOL[0]); // must not clear FAST
    assert_eq!(
        p.adkcon() & (ADKCON_FAST | ADKCON_USE_VOL[0]),
        ADKCON_FAST | ADKCON_USE_VOL[0]
    );
}

#[test]
fn adkcon_write_bit_15_clear_removes_from_mask() {
    let mut p = Paula8364::new();
    p.write_adkcon(INT_SETCLR | 0x0FFF); // SET bits 0..11
    assert_eq!(p.adkcon() & 0x0FFF, 0x0FFF);

    p.write_adkcon(0x00FF); // CLEAR low 8 bits
    assert_eq!(p.adkcon() & 0x0FFF, 0x0F00);
}

#[test]
fn adkcon_bit_15_is_not_stored() {
    let mut p = Paula8364::new();
    p.write_adkcon(0xFFFF);
    assert_eq!(
        p.adkcon() & INT_SETCLR,
        0,
        "SET flag lives only in the write"
    );
}

// ────────────────────────────────────────────────────────────────
// FAST bit — governs DSKBYTR byte pacing
// ────────────────────────────────────────────────────────────────

#[test]
fn fast_disk_bit_shortens_next_byte_arrival_cck() {
    fn ccks_until_byte_valid(fast: bool) -> u32 {
        let mut p = Paula8364::new();
        if fast {
            p.write_adkcon(INT_SETCLR | ADKCON_FAST);
        }
        p.set_dsksync(0x4489);
        let wordequal = p.note_disk_read_word(0xA1A1);
        assert!(!wordequal);
        // Drain the immediate high byte the chip makes available.
        let _ = p.read_dskbytr(0);

        for n in 1..=64u32 {
            p.tick_disk_cck();
            if p.read_dskbytr(0) & DSKBYTR_DSKBYT != 0 {
                return n;
            }
        }
        u32::MAX
    }

    assert_eq!(ccks_until_byte_valid(true), u32::from(DISK_BYTE_CCK_FAST));
    assert_eq!(ccks_until_byte_valid(false), u32::from(DISK_BYTE_CCK_SLOW));
}

// ────────────────────────────────────────────────────────────────
// Audio attach — ADKCON.USE_PER / USE_VOL
// ────────────────────────────────────────────────────────────────

#[test]
fn adkcon_use_period_bits_stored_round_trip() {
    let mut p = Paula8364::new();
    let all_per = ADKCON_USE_PER[0] | ADKCON_USE_PER[1] | ADKCON_USE_PER[2] | ADKCON_USE_PER[3];
    p.write_adkcon(INT_SETCLR | all_per);
    assert_eq!(p.adkcon() & all_per, all_per);
    p.write_adkcon(ADKCON_USE_PER[1]); // CLEAR USE1P2 only
    assert_eq!(p.adkcon() & all_per, all_per & !ADKCON_USE_PER[1]);
}

#[test]
fn adkcon_use_volume_bits_stored_round_trip() {
    let mut p = Paula8364::new();
    let all_vol = ADKCON_USE_VOL[0] | ADKCON_USE_VOL[1] | ADKCON_USE_VOL[2] | ADKCON_USE_VOL[3];
    p.write_adkcon(INT_SETCLR | all_vol);
    assert_eq!(p.adkcon() & all_vol, all_vol);
    p.write_adkcon(ADKCON_USE_VOL[0]); // CLEAR USE0V1
    assert_eq!(p.adkcon() & all_vol, all_vol & !ADKCON_USE_VOL[0]);
}

#[test]
fn adkcon_use_period_and_volume_are_independent() {
    let mut p = Paula8364::new();
    let all_per = ADKCON_USE_PER[0] | ADKCON_USE_PER[1] | ADKCON_USE_PER[2] | ADKCON_USE_PER[3];
    let all_vol = ADKCON_USE_VOL[0] | ADKCON_USE_VOL[1] | ADKCON_USE_VOL[2] | ADKCON_USE_VOL[3];
    p.write_adkcon(INT_SETCLR | all_per);
    p.write_adkcon(all_vol); // clear volume bits that were never set — noop
    assert_eq!(p.adkcon(), all_per);
    p.write_adkcon(INT_SETCLR | ADKCON_USE_VOL[0]);
    assert_eq!(p.adkcon(), all_per | ADKCON_USE_VOL[0]);
}

// ────────────────────────────────────────────────────────────────
// Reset
// ────────────────────────────────────────────────────────────────

#[test]
fn reset_clears_adkcon() {
    let mut p = Paula8364::new();
    p.write_adkcon(INT_SETCLR | 0x7FFF);
    p.reset();
    assert_eq!(p.adkcon(), 0);
}
