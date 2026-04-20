//! Phase 1 characterization tests — Paula audio channels.
//!
//! Per HRM chapter 5 (Audio Hardware). Four identical DMA-driven
//! channels, each with LC / LEN / PER / VOL / DAT. HRM minimum PER is
//! 124 CCK for playback; the register read-back preserves any written
//! value. Stereo routing: 0+3 → L, 1+2 → R. Modulator channels are
//! muted from the mix.

use commodore_paula_8364::{AudioField, IntSource, Paula8364, bits::*};

fn zero_reader(_: u32) -> u8 { 0 }

// ────────────────────────────────────────────────────────────────
// Register storage (typed API)
// ────────────────────────────────────────────────────────────────

#[test]
fn audio_registers_round_trip_per_channel() {
    let mut p = Paula8364::new();
    p.write_audio(0, AudioField::LcHi, 0x0012);
    p.write_audio(0, AudioField::LcLo, 0x3456);
    p.write_audio(0, AudioField::Len, 0x0008);
    p.write_audio(0, AudioField::Per, 500);
    p.write_audio(0, AudioField::Vol, 32);
    assert_eq!(p.read_audio(0, AudioField::LcHi), 0x0012);
    assert_eq!(p.read_audio(0, AudioField::LcLo), 0x3456);
    assert_eq!(p.read_audio(0, AudioField::Len), 0x0008);
    assert_eq!(p.read_audio(0, AudioField::Per), 500);
    assert_eq!(p.read_audio(0, AudioField::Vol), 32);
}

#[test]
fn audio_lc_low_word_masks_off_bit_0_for_word_alignment() {
    let mut p = Paula8364::new();
    p.write_audio(0, AudioField::LcLo, 0xFFFF);
    assert_eq!(p.read_audio(0, AudioField::LcLo), 0xFFFE);
}

#[test]
fn audio_vol_is_clamped_to_64_at_write_time() {
    let mut p = Paula8364::new();
    p.write_audio(0, AudioField::Vol, 0x007F);
    assert_eq!(p.read_audio(0, AudioField::Vol), 64);
    p.write_audio(0, AudioField::Vol, 0x0800); // high bits ignored
    assert_eq!(p.read_audio(0, AudioField::Vol), 0);
}

#[test]
fn audio_register_writes_to_missing_channel_are_dropped_safely() {
    let mut p = Paula8364::new();
    p.write_audio(7, AudioField::Vol, 0x0040); // out of range
    assert_eq!(p.read_audio(7, AudioField::Vol), 0);
}

// ────────────────────────────────────────────────────────────────
// AUDxPER minimum period clamp
// ────────────────────────────────────────────────────────────────

#[test]
fn audx_per_below_minimum_readback_preserves_written_value() {
    let mut p = Paula8364::new();
    p.write_audio(0, AudioField::Per, 1);
    assert_eq!(p.read_audio(0, AudioField::Per), 1);
}

#[test]
fn audx_per_below_minimum_uses_clamp_for_playback_timing() {
    let mut p = Paula8364::new();
    let dmacon = DMA_MASTER | DMA_AUD0;
    p.write_audio(0, AudioField::LcHi, 0);
    p.write_audio(0, AudioField::LcLo, 0x1000);
    p.write_audio(0, AudioField::Len, 1);
    p.write_audio(0, AudioField::Per, 1); // below minimum
    p.write_audio(0, AudioField::Vol, 64);

    let sample = |_: u32| 0x7F;
    for _ in 0..(AUDIO_MIN_PERIOD_CCK as usize - 1) {
        p.tick_audio_cck(dmacon, Some(0), true, sample);
    }
    let (left, _) = p.mix_audio_stereo();
    assert!(left.abs() < 0.01, "no output before clamped minimum elapses; got {left}");

    p.tick_audio_cck(dmacon, Some(0), true, sample);
    let (left, _) = p.mix_audio_stereo();
    assert!(left > 0.4, "output appears at the clamped minimum; got {left}");
}

// ────────────────────────────────────────────────────────────────
// DMA enable → block-start IRQ
// ────────────────────────────────────────────────────────────────

#[test]
fn dma_enable_rising_edge_raises_audx_irq() {
    let mut p = Paula8364::new();
    p.write_audio(0, AudioField::LcHi, 0);
    p.write_audio(0, AudioField::LcLo, 0x1000);
    p.write_audio(0, AudioField::Len, 0x0010);
    p.write_audio(0, AudioField::Per, 500);

    p.tick_audio_cck(DMA_MASTER | DMA_AUD0, Some(0), true, zero_reader);
    assert_ne!(p.intreq() & INT_AUD0, 0, "AUD0 IRQ fires on DMA start");
    assert_eq!(p.intreq() & (INT_AUD1 | INT_AUD2 | INT_AUD3), 0);
}

#[test]
fn channel_3_maps_to_intreq_bit_10() {
    let mut p = Paula8364::new();
    p.write_audio(3, AudioField::LcHi, 0);
    p.write_audio(3, AudioField::LcLo, 0x1000);
    p.write_audio(3, AudioField::Len, 0x0010);

    p.tick_audio_cck(DMA_MASTER | DMA_AUD3, Some(3), true, zero_reader);
    assert_ne!(p.intreq() & IntSource::Aud3.mask(), 0);
}

// ────────────────────────────────────────────────────────────────
// Stereo routing per HRM
// ────────────────────────────────────────────────────────────────

#[test]
fn channels_0_and_3_route_to_left_and_1_2_to_right() {
    let mut p = Paula8364::new();

    // Ch 1 produces +max output on the right channel only.
    p.write_audio(1, AudioField::LcHi, 0);
    p.write_audio(1, AudioField::LcLo, 0x2000);
    p.write_audio(1, AudioField::Len, 1);
    p.write_audio(1, AudioField::Vol, 64);

    let read = |_: u32| 0x7F;
    for _ in 0..AUDIO_MIN_PERIOD_CCK {
        p.tick_audio_cck(DMA_MASTER | DMA_AUD1, Some(1), true, read);
    }
    let (left, right) = p.mix_audio_stereo();
    assert!(left.abs() < 0.01, "ch 1 must not leak into left; got {left}");
    assert!(right > 0.4, "ch 1 → right; got {right}");
}

// ────────────────────────────────────────────────────────────────
// ADKCON attach modulation
// ────────────────────────────────────────────────────────────────

#[test]
fn attach_period_bit_mutes_modulator_channel_in_stereo_mix() {
    let mut p = Paula8364::new();
    p.write_adkcon(INT_SETCLR | ADKCON_USE_PER[0]);

    p.write_audio(0, AudioField::LcHi, 0);
    p.write_audio(0, AudioField::LcLo, 0x1000);
    p.write_audio(0, AudioField::Len, 1);
    p.write_audio(0, AudioField::Vol, 64);

    let read = |_: u32| 0x7F;
    for _ in 0..AUDIO_MIN_PERIOD_CCK {
        p.tick_audio_cck(DMA_MASTER | DMA_AUD0, Some(0), true, read);
    }
    let (left, _) = p.mix_audio_stereo();
    assert!(left.abs() < 0.01,
        "modulator channels don't contribute to the audio mix; got {left}");
}

#[test]
fn attach_volume_uses_channel_n_low_byte_to_set_volume_on_n_plus_1() {
    let mut p = Paula8364::new();
    p.write_adkcon(INT_SETCLR | ADKCON_USE_VOL[0]);

    p.write_audio(0, AudioField::LcHi, 0);
    p.write_audio(0, AudioField::LcLo, 0x1000);
    p.write_audio(0, AudioField::Len, 1);
    p.write_audio(0, AudioField::Vol, 64);

    // HI bytes = $7F, LO bytes = $20 → ch 1 volume → $20 once modulation fires.
    let read = |addr: u32| if addr & 1 == 0 { 0x7F } else { 0x20 };
    for _ in 0..(AUDIO_MIN_PERIOD_CCK * 2 + 4) {
        p.tick_audio_cck(DMA_MASTER | DMA_AUD0, Some(0), true, read);
    }
    let ch1_vol = p.read_audio(1, AudioField::Vol);
    assert_eq!(ch1_vol, 0x20,
        "channel 1 volume should have been written by channel 0's low-byte event; got {ch1_vol}");
}

// ────────────────────────────────────────────────────────────────
// Reset
// ────────────────────────────────────────────────────────────────

#[test]
fn reset_clears_all_audio_registers_and_irq() {
    let mut p = Paula8364::new();
    p.write_audio(0, AudioField::Vol, 64);
    p.write_audio(0, AudioField::Per, 999);
    p.raise(IntSource::Aud0);

    p.reset();

    assert_eq!(p.read_audio(0, AudioField::Vol), 0);
    assert_eq!(p.read_audio(0, AudioField::Per), AUDIO_MIN_PERIOD_CCK);
    assert_eq!(p.intreq(), 0);
}
