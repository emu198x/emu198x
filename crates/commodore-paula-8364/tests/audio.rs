//! Phase 1 characterization tests — Paula audio channels.
//!
//! Per HRM chapter 5 (Audio Hardware). Four identical DMA-driven
//! channels, each with LC / LEN / PER / VOL / DAT. HRM minimum PER is
//! 124 CCK for playback; the register read-back preserves any written
//! value. Stereo routing: 0+3 → L, 1+2 → R. Modulator channels are
//! muted from the mix.

use commodore_paula_8364::{AudioField, IntSource, Paula8364, bits::*};

fn zero_reader(_: u32) -> u8 {
    0
}

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
fn audx_per_below_minimum_clamps_byte_duration_to_the_minimum() {
    // `AUDxPER` below the HRM minimum (124) is clamped to 124 for
    // *playback timing* (the raw value is still preserved on read-back,
    // tested separately). The clamp is observable as the duration each
    // sample byte is held: with PER=1 the high byte is held for the
    // clamped minimum, not a single colour-clock.
    let mut p = Paula8364::new();
    let dmacon = DMA_MASTER | DMA_AUD0;
    p.write_audio(0, AudioField::LcHi, 0);
    p.write_audio(0, AudioField::LcLo, 0x1000);
    p.write_audio(0, AudioField::Len, 1);
    p.write_audio(0, AudioField::Per, 1); // below minimum
    p.write_audio(0, AudioField::Vol, 64);

    // Distinct high/low bytes so the byte step is observable: high byte
    // ≈ +max output, low byte ≈ silence.
    let sample = |addr: u32| if addr & 1 == 0 { 0x7F } else { 0x01 };

    // Drive past the DMA startup waits (001/101 produce no output) until
    // the high byte of the first real sample word is output.
    let mut started = false;
    for _ in 0..8 {
        p.tick_audio_cck(dmacon, Some(0), sample);
        if p.mix_audio_stereo().0 > 0.4 {
            started = true;
            break;
        }
    }
    assert!(started, "playback should begin within a few DMA slots");

    // Hold the high byte and count colour-clocks until it steps to the
    // low byte. The step must take the clamped minimum, not PER=1. (The
    // ±1 tolerance absorbs the startup phase of the period counter.)
    let mut held = 0u16;
    loop {
        p.tick_audio_cck(dmacon, Some(0), sample);
        held += 1;
        if p.mix_audio_stereo().0.abs() < 0.05 {
            break; // stepped to the low byte
        }
        assert!(held < 200, "high byte never stepped — period clamp broken");
    }
    assert!(
        (AUDIO_MIN_PERIOD_CCK - 1..=AUDIO_MIN_PERIOD_CCK).contains(&held),
        "byte duration clamps to the minimum ({AUDIO_MIN_PERIOD_CCK}); held {held} CCK"
    );
}

// ────────────────────────────────────────────────────────────────
// DMA startup state machine → first-word IRQ
// ────────────────────────────────────────────────────────────────

#[test]
fn audx_irq_fires_when_first_dma_word_arrives_not_at_the_enable_edge() {
    // Hardware: the DMA-enable edge (000→001) raises no interrupt; it
    // requests the first word. The interrupt fires when that word
    // arrives (001→101) — the startup IRQ the CPU uses to swap
    // double-buffer pointers.
    let mut p = Paula8364::new();
    p.write_audio(0, AudioField::LcHi, 0);
    p.write_audio(0, AudioField::LcLo, 0x1000);
    p.write_audio(0, AudioField::Len, 0x0010);
    p.write_audio(0, AudioField::Per, 500);

    // Enable edge with no audio slot granted this CCK: the channel
    // requests word 1 but no interrupt fires yet.
    p.tick_audio_cck(DMA_MASTER | DMA_AUD0, None, zero_reader);
    assert_eq!(
        p.intreq() & INT_AUD0,
        0,
        "no AUD0 IRQ at the DMA-enable edge — it fires on word-1 arrival"
    );

    // The channel's audio slot is granted: word 1 arrives, is discarded,
    // and the startup interrupt fires.
    p.tick_audio_cck(DMA_MASTER | DMA_AUD0, Some(0), zero_reader);
    assert_ne!(
        p.intreq() & INT_AUD0,
        0,
        "AUD0 IRQ fires when word 1 arrives"
    );
    assert_eq!(p.intreq() & (INT_AUD1 | INT_AUD2 | INT_AUD3), 0);
}

#[test]
fn channel_3_maps_to_intreq_bit_10() {
    let mut p = Paula8364::new();
    p.write_audio(3, AudioField::LcHi, 0);
    p.write_audio(3, AudioField::LcLo, 0x1000);
    p.write_audio(3, AudioField::Len, 0x0010);

    p.tick_audio_cck(DMA_MASTER | DMA_AUD3, Some(3), zero_reader);
    assert_ne!(p.intreq() & IntSource::Aud3.mask(), 0);
}

// ────────────────────────────────────────────────────────────────
// DMA startup state machine + bus-derived latency (#39)
// ────────────────────────────────────────────────────────────────

#[test]
fn dma_startup_advances_only_on_granted_audio_slots() {
    // The fetched word arrives on the CCK Agnus grants the channel's
    // audio slot — that grant *is* the bus latency (no fixed countdown).
    // With no slot granted the state machine cannot advance: no IRQ, no
    // output, however many colour-clocks elapse.
    let mut p = Paula8364::new();
    p.write_audio(0, AudioField::LcHi, 0);
    p.write_audio(0, AudioField::LcLo, 0x1000);
    p.write_audio(0, AudioField::Len, 4);
    p.write_audio(0, AudioField::Vol, 64);
    let sample = |_: u32| 0x7F;

    for _ in 0..500 {
        p.tick_audio_cck(DMA_MASTER | DMA_AUD0, None, sample);
    }
    assert_eq!(p.intreq() & INT_AUD0, 0, "no IRQ without a granted slot");
    assert!(
        p.mix_audio_stereo().0.abs() < 0.01,
        "no output without a granted slot"
    );

    // First grant: word 1 arrives → startup IRQ.
    p.tick_audio_cck(DMA_MASTER | DMA_AUD0, Some(0), sample);
    assert_ne!(p.intreq() & INT_AUD0, 0, "word-1 arrival raises the IRQ");
}

#[test]
fn dma_startup_outputs_high_byte_of_first_sample_word() {
    // Word 1 is a dummy fetch — discarded, and the location pointer is
    // reset (AUDxDSR). The real first sample word is fetched second from
    // the *start* of the buffer, and playback opens on its high byte
    // (penhi at 101→010). A missed pointer-reset would surface the next
    // word's high byte instead.
    let mut p = Paula8364::new();
    p.write_audio(0, AudioField::LcHi, 0);
    p.write_audio(0, AudioField::LcLo, 0x1000);
    p.write_audio(0, AudioField::Len, 4);
    p.write_audio(0, AudioField::Vol, 64);
    let sample = |addr: u32| match addr & !1 {
        0x1000 => {
            if addr & 1 == 0 {
                0xAA
            } else {
                0xBB
            }
        }
        _ => {
            if addr & 1 == 0 {
                0xCC
            } else {
                0xDD
            }
        }
    };

    // Two granted slots: word 1 (discarded) then word 2 (real data).
    p.tick_audio_cck(DMA_MASTER | DMA_AUD0, Some(0), sample);
    p.tick_audio_cck(DMA_MASTER | DMA_AUD0, Some(0), sample);

    let snap = p.audio_state(0).expect("channel 0 exists");
    assert_eq!(
        snap.sample, 0xAAu8 as i8,
        "playback opens on the high byte of the word at the buffer start"
    );
}

#[test]
fn dma_disabled_during_startup_wait_returns_to_idle_without_irq() {
    let mut p = Paula8364::new();
    p.write_audio(0, AudioField::LcHi, 0);
    p.write_audio(0, AudioField::LcLo, 0x1000);
    p.write_audio(0, AudioField::Len, 4);
    p.write_audio(0, AudioField::Vol, 64);
    let sample = |_: u32| 0x7F;

    // Enter the startup wait but never grant a slot (stays in 001).
    p.tick_audio_cck(DMA_MASTER | DMA_AUD0, None, sample);
    // Disable DMA before any word arrives.
    p.tick_audio_cck(DMA_MASTER, None, sample);
    // Granting slots now must not resurrect the channel.
    for _ in 0..8 {
        p.tick_audio_cck(DMA_MASTER, Some(0), sample);
    }
    assert_eq!(
        p.intreq() & INT_AUD0,
        0,
        "no IRQ after disabling DMA mid-startup"
    );
    assert!(
        p.mix_audio_stereo().0.abs() < 0.01,
        "no output after disabling DMA mid-startup"
    );
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
        p.tick_audio_cck(DMA_MASTER | DMA_AUD1, Some(1), read);
    }
    let (left, right) = p.mix_audio_stereo();
    assert!(
        left.abs() < 0.01,
        "ch 1 must not leak into left; got {left}"
    );
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
        p.tick_audio_cck(DMA_MASTER | DMA_AUD0, Some(0), read);
    }
    let (left, _) = p.mix_audio_stereo();
    assert!(
        left.abs() < 0.01,
        "modulator channels don't contribute to the audio mix; got {left}"
    );
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
        p.tick_audio_cck(DMA_MASTER | DMA_AUD0, Some(0), read);
    }
    let ch1_vol = p.read_audio(1, AudioField::Vol);
    assert_eq!(
        ch1_vol, 0x20,
        "channel 1 volume should have been written by channel 0's low-byte event; got {ch1_vol}"
    );
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
