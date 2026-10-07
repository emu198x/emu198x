//! Phase 1 characterization tests — Paula audio channels.
//!
//! Per HRM chapter 5 (Audio Hardware). Four identical DMA-driven
//! channels, each with LC / LEN / PER / VOL / DAT. HRM minimum PER is
//! 124 CCK for playback; the register read-back preserves any written
//! value. Stereo routing: 1+2 → L, 0+3 → R. Modulator channels are
//! muted from the mix.

use emu198x_commodore_paula_8364::{AudioField, IntSource, Paula8364, bits::*};

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
// 6-bit volume DAC (#38)
// ────────────────────────────────────────────────────────────────

#[test]
fn channel_volume_scales_output_linearly_across_the_6_bit_range() {
    // Characterize the current scalar mixer: its gain equals the average of
    // the HRM's 64-step PWM duty cycle for a constant sample and volume.
    // This does not establish physical pulse phase or gain-write timing.
    // See volume-output-probe for the separately measured PWM discrepancy.
    fn output_at_volume(vol: u16) -> f32 {
        let mut p = Paula8364::new();
        p.write_audio(0, AudioField::LcHi, 0);
        p.write_audio(0, AudioField::LcLo, 0x1000);
        p.write_audio(0, AudioField::Len, 1);
        p.write_audio(0, AudioField::Vol, vol);
        let sample = |_: u32| 0x7F; // +127 in both bytes
        // Drive past the DMA startup waits until playback begins.
        for _ in 0..8 {
            p.tick_audio_cck(DMA_MASTER | DMA_AUD0, Some(0), sample);
        }
        p.mix_audio_stereo().1 // channel 0 → right
    }

    let full = output_at_volume(64);
    let half = output_at_volume(32);
    let zero = output_at_volume(0);
    let over = output_at_volume(80); // > 64 clamps to 64

    assert!(full > 0.4, "full volume produces output; got {full}");
    assert!(zero.abs() < 1e-6, "zero volume is silent; got {zero}");
    assert!(
        (half / full - 0.5).abs() < 0.02,
        "volume 32 is half of volume 64; got ratio {}",
        half / full
    );
    assert!(
        (over - full).abs() < 1e-6,
        "volume above 64 clamps to 64; over={over} full={full}"
    );
}

#[test]
fn forced_full_volume_preserves_nominal_signed_amplitude_and_routing() {
    // HRM ch.5 describes signed digitised amplitudes. Pinned WinUAE's
    // DO_CHANNEL_1 and vAmiga's penhi/penlo retain that nominal scale.
    // Both volume encodings force every PWM step on, avoiding phase claims.
    let mut cases = 0;
    let mut mismatches = 0;
    for line in
        include_str!("../../../test-data/commodore/amiga/paula-audio/amplitude-probe/winuae.csv")
            .lines()
    {
        let row: Vec<i32> = line
            .split(',')
            .map(|value| value.parse().expect("reference integer"))
            .collect();
        assert_eq!(row.len(), 4);
        let channel = row[0] as u8;
        let volume = row[1] as u16;
        let encoded = row[2] as u16;
        let mut p = Paula8364::new();
        p.write_audio(channel, AudioField::Per, 1000);
        p.write_audio(channel, AudioField::Vol, volume);
        p.write_audio(channel, AudioField::Dat, encoded * 0x0101);
        // Reference units are signed byte × 64. Native full scale is ±1,
        // with two channels sharing each side at half gain.
        let nominal = row[3] as f32 / 16_384.0;
        let expected = if channel == 1 || channel == 2 {
            (nominal, 0.0)
        } else {
            (0.0, nominal)
        };
        mismatches += usize::from(p.mix_audio_stereo() != expected);
        cases += 1;
    }
    assert_eq!(cases, 2048);
    assert_eq!(
        mismatches, 0,
        "nominal amplitude or routing differs across {cases} cases"
    );
}

// ────────────────────────────────────────────────────────────────
// AUDxPER values below the recommended DMA period
// ────────────────────────────────────────────────────────────────

#[test]
fn audx_per_below_minimum_readback_preserves_written_value() {
    let mut p = Paula8364::new();
    p.write_audio(0, AudioField::Per, 1);
    assert_eq!(p.read_audio(0, AudioField::Per), 1);
}

#[test]
fn audx_per_below_recommended_dma_period_uses_programmed_byte_duration() {
    // The DMA recommendation does not clamp the counter. Keep data available
    // here so this test measures the sample hold rather than DMA starvation.
    const PERIOD: u16 = 60;
    let mut p = Paula8364::new();
    let dmacon = DMA_MASTER | DMA_AUD0;
    p.write_audio(0, AudioField::LcHi, 0);
    p.write_audio(0, AudioField::LcLo, 0x1000);
    p.write_audio(0, AudioField::Len, 1);
    p.write_audio(0, AudioField::Per, PERIOD);
    p.write_audio(0, AudioField::Vol, 64);

    // Distinct high/low bytes so the byte step is observable: high byte
    // ≈ +max output, low byte ≈ silence.
    let sample = |addr: u32| if addr & 1 == 0 { 0x7F } else { 0x01 };

    // Drive past the DMA startup waits (001/101 produce no output) until
    // the high byte of the first real sample word is output.
    let mut started = false;
    for _ in 0..8 {
        p.tick_audio_cck(dmacon, Some(0), sample);
        if p.mix_audio_stereo().1 > 0.4 {
            started = true;
            break;
        }
    }
    assert!(started, "playback should begin within a few DMA slots");

    // The combined component helper also clocks the output on the service
    // call, so the first observed sample has already used up one tick.
    let mut held = 0u16;
    loop {
        p.tick_audio_cck(dmacon, Some(0), sample);
        held += 1;
        if p.mix_audio_stereo().1.abs() < 0.05 {
            break; // stepped to the low byte
        }
        assert!(
            held < 200,
            "high byte never stepped — period counter stalled"
        );
    }
    assert!(
        (PERIOD - 1..=PERIOD).contains(&held),
        "byte duration uses the programmed period ({PERIOD}); held {held} CCK"
    );
}

// ────────────────────────────────────────────────────────────────
// DMA startup state machine → first-word IRQ
// ────────────────────────────────────────────────────────────────

#[test]
fn audx_irq_arrives_one_cck_after_the_first_dma_word() {
    // Hardware: the DMA-enable edge (000→001) raises no interrupt; it
    // requests the first word. The interrupt is requested when that word
    // arrives (001→101), delivered one CCK later. The CPU uses it to swap
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
    // and the startup interrupt enters its delivery stage.
    p.tick_audio_cck(DMA_MASTER | DMA_AUD0, Some(0), zero_reader);
    assert_eq!(p.intreq() & INT_AUD0, 0);
    assert!(p.audio_diagnostic_snapshot().channels[0].interrupt_request_pending);
    p.tick_audio_cck(DMA_MASTER | DMA_AUD0, None, zero_reader);
    assert!(!p.audio_diagnostic_snapshot().channels[0].interrupt_request_pending);
    assert_ne!(
        p.intreq() & INT_AUD0,
        0,
        "AUD0 IRQ arrives one CCK after word 1"
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
    assert_eq!(p.intreq() & IntSource::Aud3.mask(), 0);
    p.tick_audio_cck(DMA_MASTER | DMA_AUD3, None, zero_reader);
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
        p.mix_audio_stereo().1.abs() < 0.01,
        "no output without a granted slot"
    );

    // First grant requests the startup IRQ; delivery follows one CCK later.
    p.tick_audio_cck(DMA_MASTER | DMA_AUD0, Some(0), sample);
    assert_eq!(p.intreq() & INT_AUD0, 0);
    p.tick_audio_cck(DMA_MASTER | DMA_AUD0, None, sample);
    assert_ne!(p.intreq() & INT_AUD0, 0, "startup IRQ arrives next CCK");
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
        p.mix_audio_stereo().1.abs() < 0.01,
        "no output after disabling DMA mid-startup"
    );
}

// ────────────────────────────────────────────────────────────────
// Stereo routing per HRM
// ────────────────────────────────────────────────────────────────

#[test]
fn channels_1_and_2_route_to_left_and_0_3_to_right() {
    fn output(channel: u8) -> (f32, f32) {
        let mut p = Paula8364::new();
        p.write_audio(channel, AudioField::LcHi, 0);
        p.write_audio(channel, AudioField::LcLo, 0x2000);
        p.write_audio(channel, AudioField::Len, 1);
        p.write_audio(channel, AudioField::Vol, 64);

        let read = |_: u32| 0x7F;
        for _ in 0..AUDIO_MIN_PERIOD_CCK {
            p.tick_audio_cck(DMA_MASTER | (DMA_AUD0 << channel), Some(channel), read);
        }
        p.mix_audio_stereo()
    }

    for channel in [1, 2] {
        let (left, right) = output(channel);
        assert!(left > 0.4, "ch {channel} → left; got {left}");
        assert!(
            right.abs() < 0.01,
            "ch {channel} must not leak into right; got {right}"
        );
    }
    for channel in [0, 3] {
        let (left, right) = output(channel);
        assert!(right > 0.4, "ch {channel} → right; got {right}");
        assert!(
            left.abs() < 0.01,
            "ch {channel} must not leak into left; got {left}"
        );
    }
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
    let (_, right) = p.mix_audio_stereo();
    assert!(
        right.abs() < 0.01,
        "modulator channels don't contribute to the audio mix; got {right}"
    );
}

#[test]
fn attach_volume_uses_low_seven_bits_of_the_holding_word() {
    let mut p = Paula8364::new();
    p.write_adkcon(INT_SETCLR | ADKCON_USE_VOL[0]);

    p.write_audio(0, AudioField::LcHi, 0);
    p.write_audio(0, AudioField::LcLo, 0x1000);
    p.write_audio(0, AudioField::Len, 1);
    p.write_audio(0, AudioField::Vol, 64);

    // The holding word is $7F20; its low seven bits select volume $20.
    let read = |addr: u32| if addr & 1 == 0 { 0x7F } else { 0x20 };
    for _ in 0..(AUDIO_MIN_PERIOD_CCK * 2 + 4) {
        p.tick_audio_cck(DMA_MASTER | DMA_AUD0, Some(0), read);
    }
    let ch1_vol = p.read_audio(1, AudioField::Vol);
    assert_eq!(
        ch1_vol, 0x20,
        "channel 1 volume should use the holding word's low seven bits; got {ch1_vol}"
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
