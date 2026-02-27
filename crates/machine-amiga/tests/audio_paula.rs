use std::fs;
use std::path::Path;

use machine_amiga::memory::ROM_BASE;
use machine_amiga::{Amiga, TICKS_PER_CCK};

const REG_DMACON: u16 = 0x096;
const REG_DDFSTRT: u16 = 0x092;
const REG_DDFSTOP: u16 = 0x094;
const REG_ADKCON: u16 = 0x09E;
const REG_COP1LCH: u16 = 0x080;
const REG_COP1LCL: u16 = 0x082;
const REG_AUD0LCH: u16 = 0x0A0;
const REG_AUD0LCL: u16 = 0x0A2;
const REG_AUD0LEN: u16 = 0x0A4;
const REG_AUD0PER: u16 = 0x0A6;
const REG_AUD0VOL: u16 = 0x0A8;
const REG_AUD0DAT: u16 = 0x0AA;
const REG_BPLCON0: u16 = 0x100;
const AUD_STRIDE: u16 = 0x10;
const AUD_REG_PER_OFFSET: u16 = 0x06;
const AUD_REG_VOL_OFFSET: u16 = 0x08;
const AUD_REG_DAT_OFFSET: u16 = 0x0A;

const DMACON_DMAEN: u16 = 0x0200;
const DMACON_AUD0EN: u16 = 0x0001;
const DMACON_AUD1EN: u16 = 0x0002;
const DMACON_COPEN: u16 = 0x0080;
const DMACON_SPREN: u16 = 0x0020;
const DMACON_BPLEN: u16 = 0x0100;
const INTREQ_AUD0: u16 = 0x0080;
const ADKCON_SETCLR: u16 = 0x8000;
const ADKCON_USE0V1: u16 = 0x0001;
const ADKCON_USE0P1: u16 = 0x0010;

fn make_test_amiga() -> Amiga {
    let mut rom = vec![0u8; 256 * 1024];

    // Initial SSP = top of 512K chip RAM (minus a little headroom)
    let ssp = 0x0007FFF0u32;
    rom[0] = (ssp >> 24) as u8;
    rom[1] = (ssp >> 16) as u8;
    rom[2] = (ssp >> 8) as u8;
    rom[3] = ssp as u8;

    // Initial PC = Kickstart ROM base + 8
    let pc = ROM_BASE + 8;
    rom[4] = (pc >> 24) as u8;
    rom[5] = (pc >> 16) as u8;
    rom[6] = (pc >> 8) as u8;
    rom[7] = pc as u8;

    // $F80008: BRA.S *  (tight loop, no custom chip traffic)
    rom[8] = 0x60;
    rom[9] = 0xFE;

    Amiga::new(rom)
}

fn tick_ccks(amiga: &mut Amiga, ccks: u32) {
    for _ in 0..ccks {
        for _ in 0..TICKS_PER_CCK {
            amiga.tick();
        }
    }
}

fn tick_until<F>(amiga: &mut Amiga, max_ccks: u32, mut pred: F) -> Option<u32>
where
    F: FnMut(&Amiga) -> bool,
{
    for elapsed in 0..=max_ccks {
        if pred(amiga) {
            return Some(elapsed);
        }
        if elapsed != max_ccks {
            tick_ccks(amiga, 1);
        }
    }
    None
}

fn aud0_irq_pending(amiga: &Amiga) -> bool {
    (amiga.paula.intreq & INTREQ_AUD0) != 0
}

fn clear_aud0_irq(amiga: &mut Amiga) {
    amiga.paula.intreq &= !INTREQ_AUD0;
}

fn write_aud0_lc(amiga: &mut Amiga, addr: u32) {
    amiga.write_custom_reg(REG_AUD0LCH, (addr >> 16) as u16);
    amiga.write_custom_reg(REG_AUD0LCL, (addr & 0xFFFF) as u16);
}

fn write_cop1_lc(amiga: &mut Amiga, addr: u32) {
    amiga.write_custom_reg(REG_COP1LCH, (addr >> 16) as u16);
    amiga.write_custom_reg(REG_COP1LCL, (addr & 0xFFFF) as u16);
}

fn configure_aud0_dma(amiga: &mut Amiga, sample_addr: u32, len_words: u16, period: u16, vol: u16) {
    write_aud0_lc(amiga, sample_addr);
    amiga.write_custom_reg(REG_AUD0LEN, len_words);
    amiga.write_custom_reg(REG_AUD0PER, period);
    amiga.write_custom_reg(REG_AUD0VOL, vol);
}

fn aud_reg(channel: u8, offset: u16) -> u16 {
    REG_AUD0LCH + u16::from(channel) * AUD_STRIDE + offset
}

fn write_aud_direct(amiga: &mut Amiga, channel: u8, period: u16, vol: u16, hi: u8, lo: u8) {
    amiga.write_custom_reg(aud_reg(channel, AUD_REG_PER_OFFSET), period);
    amiga.write_custom_reg(aud_reg(channel, AUD_REG_VOL_OFFSET), vol);
    amiga.write_custom_reg(
        aud_reg(channel, AUD_REG_DAT_OFFSET),
        (u16::from(hi) << 8) | u16::from(lo),
    );
}

#[test]
fn aud0_dma_irq_cadence_and_audio_output() {
    let mut amiga = make_test_amiga();
    let sample_addr = 0x0000_2000u32;

    // One repeating word: +127 then -128 on the left channel (AUD0).
    amiga.memory.write_byte(sample_addr, 0x7F);
    amiga.memory.write_byte(sample_addr + 1, 0x80);

    configure_aud0_dma(&mut amiga, sample_addr, 1, 124, 64);
    amiga.write_custom_reg(REG_DMACON, 0x8000 | DMACON_DMAEN | DMACON_AUD0EN);

    // DMA enable edge triggers the first AUD0 block interrupt on the next CCK.
    assert!(!aud0_irq_pending(&amiga));
    tick_ccks(&mut amiga, 1);
    assert!(
        aud0_irq_pending(&amiga),
        "AUD0 IRQ should fire on DMA start"
    );
    clear_aud0_irq(&mut amiga);

    // With LEN=1, the next AUD0 interrupt should occur when the block reloads
    // on the following AUD0 DMA slot (line cadence): exactly 234 CCKs later
    // from the first interrupt point with the current Agnus start state.
    let mut delta_ccks = 0u32;
    while delta_ccks < 400 && !aud0_irq_pending(&amiga) {
        tick_ccks(&mut amiga, 1);
        delta_ccks += 1;
    }
    assert!(
        aud0_irq_pending(&amiga),
        "AUD0 IRQ should fire on block reload"
    );
    assert_eq!(delta_ccks, 234, "unexpected AUD0 DMA IRQ cadence");

    // Run long enough to accumulate host-facing audio samples.
    tick_ccks(&mut amiga, 4_000);
    let audio = amiga.take_audio_buffer();
    assert!(
        audio.len() >= 32 && audio.len() % 2 == 0,
        "expected interleaved stereo audio samples"
    );

    let mut saw_left_nonzero = false;
    let mut right_max_abs = 0.0f32;
    for frame in audio.chunks_exact(2) {
        let left = frame[0];
        let right = frame[1];
        if left.abs() > 0.1 {
            saw_left_nonzero = true;
        }
        right_max_abs = right_max_abs.max(right.abs());
    }
    assert!(
        saw_left_nonzero,
        "expected non-zero left-channel output from AUD0"
    );
    assert!(
        right_max_abs < 0.01,
        "AUD0 should not drive right channel (max abs right={right_max_abs})"
    );
}

#[test]
fn aud0_direct_mode_irq_after_two_samples() {
    let mut amiga = make_test_amiga();

    amiga.write_custom_reg(REG_AUD0PER, 124);
    amiga.write_custom_reg(REG_AUD0DAT, 0x7F80);
    clear_aud0_irq(&mut amiga);
    assert!(!aud0_irq_pending(&amiga), "no IRQ on AUD0DAT write");

    tick_ccks(&mut amiga, 124);
    assert!(
        !aud0_irq_pending(&amiga),
        "no IRQ after first sample (upper byte) output"
    );

    tick_ccks(&mut amiga, 124);
    assert!(
        aud0_irq_pending(&amiga),
        "IRQ after second sample (lower byte) output"
    );
}

#[test]
fn aud0_direct_mode_period_clamp_affects_irq_timing() {
    let mut amiga = make_test_amiga();

    // Below the hardware minimum; playback timing should clamp to 124 CCK.
    amiga.write_custom_reg(REG_AUD0PER, 1);
    amiga.write_custom_reg(REG_AUD0DAT, 0x7F80);
    clear_aud0_irq(&mut amiga);

    tick_ccks(&mut amiga, 247);
    assert!(
        !aud0_irq_pending(&amiga),
        "period clamp should prevent early direct-mode IRQ"
    );

    tick_ccks(&mut amiga, 1);
    assert!(
        aud0_irq_pending(&amiga),
        "direct-mode IRQ should fire after 248 CCKs with clamped period"
    );
}

#[test]
fn aud1_and_aud2_route_to_right_channel() {
    let mut amiga = make_test_amiga();

    write_aud_direct(&mut amiga, 1, 124, 64, 0x7F, 0x00);
    write_aud_direct(&mut amiga, 2, 124, 64, 0x7F, 0x00);

    tick_ccks(&mut amiga, 124);
    let (left, right) = amiga.paula.mix_audio_stereo();

    assert!(
        left.abs() < 0.01,
        "AUD1/AUD2 should not drive left (left={left})"
    );
    assert!(
        right > 0.9,
        "AUD1+AUD2 should strongly drive right (right={right})"
    );
}

#[test]
fn aud3_routes_to_left_channel_like_aud0() {
    let mut amiga = make_test_amiga();

    write_aud_direct(&mut amiga, 3, 124, 64, 0x7F, 0x00);

    tick_ccks(&mut amiga, 124);
    let (left, right) = amiga.paula.mix_audio_stereo();

    assert!(left > 0.45, "AUD3 should drive left (left={left})");
    assert!(
        right.abs() < 0.01,
        "AUD3 should not drive right (right={right})"
    );
}

#[test]
fn same_side_channels_average_and_can_cancel() {
    let mut amiga = make_test_amiga();

    // Left side = AUD0 + AUD3. Equal and opposite values should cancel.
    write_aud_direct(&mut amiga, 0, 124, 64, 0x40, 0x00); // +64
    write_aud_direct(&mut amiga, 3, 124, 64, 0xC0, 0x00); // -64
    // Right side = AUD1 + AUD2. Two equal positives should sum before averaging.
    write_aud_direct(&mut amiga, 1, 124, 64, 0x40, 0x00); // +64
    write_aud_direct(&mut amiga, 2, 124, 64, 0x40, 0x00); // +64

    tick_ccks(&mut amiga, 124);
    let (left, right) = amiga.paula.mix_audio_stereo();

    assert!(
        left.abs() < 0.02,
        "AUD0/AUD3 equal-opposite values should cancel on left (left={left})"
    );
    assert!(
        (right - 0.5).abs() < 0.05,
        "AUD1/AUD2 averaging should produce ~0.5 on right (right={right})"
    );
}

#[test]
fn aud0_dma_volume_modulation_updates_aud1_and_mutes_aud0_output() {
    let mut amiga = make_test_amiga();
    let sample_addr = 0x0000_2400u32;

    // AUD0 modulator data word: high byte would be audible if not muted,
    // low 7 bits should become AUD1VOL.
    amiga.memory.write_byte(sample_addr, 0x7F);
    amiga.memory.write_byte(sample_addr + 1, 0x32);

    configure_aud0_dma(&mut amiga, sample_addr, 1, 124, 64);
    amiga.write_custom_reg(aud_reg(1, AUD_REG_VOL_OFFSET), 5);
    amiga.write_custom_reg(REG_ADKCON, ADKCON_SETCLR | ADKCON_USE0V1);
    amiga.write_custom_reg(REG_DMACON, 0x8000 | DMACON_DMAEN | DMACON_AUD0EN);

    // After one sample period only one byte has been output, so modulation
    // should not yet apply.
    tick_ccks(&mut amiga, 124);
    assert_eq!(
        amiga
            .paula
            .read_audio_register(aud_reg(1, AUD_REG_VOL_OFFSET)),
        Some(5),
        "DMA-driven modulation should wait until the full word completes"
    );

    let changed_at = tick_until(&mut amiga, 2_000, |a| {
        a.paula.read_audio_register(aud_reg(1, AUD_REG_VOL_OFFSET)) == Some(0x32)
    });
    assert!(
        changed_at.is_some(),
        "AUD1VOL should be modulated by AUD0 DMA data within a bounded time"
    );

    let (left, right) = amiga.paula.mix_audio_stereo();
    assert!(
        left.abs() < 0.01,
        "AUD0 should be muted while used as a modulator (left={left})"
    );
    assert!(
        right.abs() < 0.01,
        "AUD1 has no sample loaded, so modulation should not create audible output by itself (right={right})"
    );
}

#[test]
fn aud0_dma_combined_modulation_alternates_aud1_period_then_volume() {
    let mut amiga = make_test_amiga();
    let sample_addr = 0x0000_2600u32;

    // Three modulation words: period, volume, period.
    let words = [0x0123u16, 0x0040u16, 0x0002u16];
    for (i, word) in words.into_iter().enumerate() {
        let addr = sample_addr + (i as u32) * 2;
        amiga.memory.write_byte(addr, (word >> 8) as u8);
        amiga.memory.write_byte(addr + 1, word as u8);
    }

    // Use a slower source period so this test checks ordering only, not DMA-slot
    // throughput limits of the current machine model.
    configure_aud0_dma(&mut amiga, sample_addr, 3, 500, 64);
    amiga.write_custom_reg(aud_reg(1, AUD_REG_PER_OFFSET), 500);
    amiga.write_custom_reg(aud_reg(1, AUD_REG_VOL_OFFSET), 7);
    amiga.write_custom_reg(REG_ADKCON, ADKCON_SETCLR | ADKCON_USE0P1 | ADKCON_USE0V1);
    amiga.write_custom_reg(REG_DMACON, 0x8000 | DMACON_DMAEN | DMACON_AUD0EN);

    let period1_at = tick_until(&mut amiga, 6_000, |a| {
        a.paula.read_audio_register(aud_reg(1, AUD_REG_PER_OFFSET)) == Some(0x0123)
    });
    assert!(
        period1_at.is_some(),
        "combined modulation should first update AUD1PER"
    );
    assert_eq!(
        amiga
            .paula
            .read_audio_register(aud_reg(1, AUD_REG_VOL_OFFSET)),
        Some(7),
        "first combined modulation word should not touch AUD1VOL"
    );

    let vol_at = tick_until(&mut amiga, 6_000, |a| {
        a.paula.read_audio_register(aud_reg(1, AUD_REG_VOL_OFFSET)) == Some(64)
    });
    assert!(
        vol_at.is_some(),
        "combined modulation should then update AUD1VOL"
    );

    let period2_at = tick_until(&mut amiga, 6_000, |a| {
        a.paula.read_audio_register(aud_reg(1, AUD_REG_PER_OFFSET)) == Some(2)
    });
    assert!(
        period2_at.is_some(),
        "combined modulation should alternate back to AUD1PER"
    );
}

#[test]
fn aud0_dma_combined_modulation_uses_both_transitions_for_cadence() {
    let mut amiga = make_test_amiga();
    let sample_addr = 0x0000_2C00u32;

    // Three modulation words: period, volume, period.
    let words = [0x0123u16, 0x0040u16, 0x0002u16];
    for (i, word) in words.into_iter().enumerate() {
        let addr = sample_addr + (i as u32) * 2;
        amiga.memory.write_byte(addr, (word >> 8) as u8);
        amiga.memory.write_byte(addr + 1, word as u8);
    }

    // Slower period avoids DMA-slot starvation in this machine model
    // and lets us assert transition-to-transition cadence deterministically.
    configure_aud0_dma(&mut amiga, sample_addr, 3, 500, 64);
    amiga.write_custom_reg(aud_reg(1, AUD_REG_PER_OFFSET), 700);
    amiga.write_custom_reg(aud_reg(1, AUD_REG_VOL_OFFSET), 7);
    amiga.write_custom_reg(REG_ADKCON, ADKCON_SETCLR | ADKCON_USE0P1 | ADKCON_USE0V1);
    amiga.write_custom_reg(REG_DMACON, 0x8000 | DMACON_DMAEN | DMACON_AUD0EN);

    let first_period = tick_until(&mut amiga, 20_000, |a| {
        a.paula.read_audio_register(aud_reg(1, AUD_REG_PER_OFFSET)) == Some(0x0123)
    });
    assert!(
        first_period.is_some(),
        "combined modulation should first update AUD1PER"
    );

    let volume_delta = tick_until(&mut amiga, 2_000, |a| {
        a.paula.read_audio_register(aud_reg(1, AUD_REG_VOL_OFFSET)) == Some(64)
    });
    assert_eq!(
        volume_delta,
        Some(500),
        "combined attach should update volume one source transition later (500 CCK)"
    );

    let period_delta = tick_until(&mut amiga, 2_000, |a| {
        a.paula.read_audio_register(aud_reg(1, AUD_REG_PER_OFFSET)) == Some(2)
    });
    assert_eq!(
        period_delta,
        Some(500),
        "combined attach should return to period on the next source transition (500 CCK)"
    );
}

#[test]
fn aud0_dma_combined_modulation_high_rate_degrades_but_preserves_order() {
    let mut amiga = make_test_amiga();
    let sample_addr = 0x0000_2E00u32;

    // Four modulation words: period, volume, period, volume.
    let words = [0x0123u16, 0x0040u16, 0x0002u16, 0x0020u16];
    for (i, word) in words.into_iter().enumerate() {
        let addr = sample_addr + (i as u32) * 2;
        amiga.memory.write_byte(addr, (word >> 8) as u8);
        amiga.memory.write_byte(addr + 1, word as u8);
    }

    // Fastest practical source period. Combined attach wants one word per 124 CCK,
    // which exceeds per-line DMA slot throughput, so we expect delayed (but ordered)
    // modulation updates.
    configure_aud0_dma(&mut amiga, sample_addr, 4, 124, 64);
    amiga.write_custom_reg(aud_reg(1, AUD_REG_PER_OFFSET), 700);
    amiga.write_custom_reg(aud_reg(1, AUD_REG_VOL_OFFSET), 7);
    amiga.write_custom_reg(REG_ADKCON, ADKCON_SETCLR | ADKCON_USE0P1 | ADKCON_USE0V1);
    amiga.write_custom_reg(REG_DMACON, 0x8000 | DMACON_DMAEN | DMACON_AUD0EN);

    let first_period = tick_until(&mut amiga, 20_000, |a| {
        a.paula.read_audio_register(aud_reg(1, AUD_REG_PER_OFFSET)) == Some(0x0123)
    });
    assert!(
        first_period.is_some(),
        "high-rate combined attach should still produce the first period modulation"
    );

    let volume_delta = tick_until(&mut amiga, 20_000, |a| {
        a.paula.read_audio_register(aud_reg(1, AUD_REG_VOL_OFFSET)) == Some(64)
    });
    assert!(
        volume_delta.is_some(),
        "high-rate combined attach should eventually produce the matching volume modulation"
    );
    let volume_delta = volume_delta.unwrap();
    assert!(
        volume_delta > 124,
        "volume modulation should be delayed beyond one source period under DMA slot pressure (delta={volume_delta})"
    );

    let period_delta = tick_until(&mut amiga, 20_000, |a| {
        a.paula.read_audio_register(aud_reg(1, AUD_REG_PER_OFFSET)) == Some(2)
    });
    assert!(
        period_delta.is_some(),
        "high-rate combined attach should eventually reach the next period word"
    );

    let volume2_delta = tick_until(&mut amiga, 20_000, |a| {
        a.paula.read_audio_register(aud_reg(1, AUD_REG_VOL_OFFSET)) == Some(32)
    });
    assert!(
        volume2_delta.is_some(),
        "high-rate combined attach should preserve period/volume order across repeated starvation"
    );
}

#[test]
fn aud0_dma_combined_modulation_high_rate_with_agnus_dma_contention_preserves_order() {
    fn run_case(enable_agnus_contention: bool) {
        let mut amiga = make_test_amiga();
        let sample_addr = 0x0000_3000u32;

        let words = [0x0123u16, 0x0040u16, 0x0002u16, 0x0020u16];
        for (i, word) in words.into_iter().enumerate() {
            let addr = sample_addr + (i as u32) * 2;
            amiga.memory.write_byte(addr, (word >> 8) as u8);
            amiga.memory.write_byte(addr + 1, word as u8);
        }

        configure_aud0_dma(&mut amiga, sample_addr, 4, 124, 64);
        amiga.write_custom_reg(aud_reg(1, AUD_REG_PER_OFFSET), 700);
        amiga.write_custom_reg(aud_reg(1, AUD_REG_VOL_OFFSET), 7);
        amiga.write_custom_reg(REG_ADKCON, ADKCON_SETCLR | ADKCON_USE0P1 | ADKCON_USE0V1);

        let mut dmacon = 0x8000 | DMACON_DMAEN | DMACON_AUD0EN;
        if enable_agnus_contention {
            amiga.write_custom_reg(REG_DDFSTRT, 0x001C);
            amiga.write_custom_reg(REG_DDFSTOP, 0x00D8);
            amiga.write_custom_reg(REG_BPLCON0, 6 << 12); // 6 bitplanes
            // Channel 0 audio return (~14 CCK after the AUD0 slot) lands in the
            // early-scanline sprite DMA region, so enable sprite DMA to create a
            // deterministic contention signal for return-latency progress. We
            // also enable bitplane DMA to keep display-load coexistence covered.
            dmacon |= DMACON_BPLEN | DMACON_SPREN;
        }
        amiga.write_custom_reg(REG_DMACON, dmacon);

        let first_period = tick_until(&mut amiga, 20_000, |a| {
            a.paula.read_audio_register(aud_reg(1, AUD_REG_PER_OFFSET)) == Some(0x0123)
        });
        assert!(
            first_period.is_some(),
            "combined attach should produce first period modulation"
        );

        let volume_delta = tick_until(&mut amiga, 20_000, |a| {
            a.paula.read_audio_register(aud_reg(1, AUD_REG_VOL_OFFSET)) == Some(64)
        });
        assert!(
            volume_delta.is_some(),
            "combined attach should produce matching volume modulation"
        );
        let _volume_delta = volume_delta.unwrap();

        let period_delta = tick_until(&mut amiga, 20_000, |a| {
            a.paula.read_audio_register(aud_reg(1, AUD_REG_PER_OFFSET)) == Some(2)
        });
        assert!(
            period_delta.is_some(),
            "combined attach should preserve period/volume order"
        );

        let volume2_delta = tick_until(&mut amiga, 20_000, |a| {
            a.paula.read_audio_register(aud_reg(1, AUD_REG_VOL_OFFSET)) == Some(32)
        });
        assert!(
            volume2_delta.is_some(),
            "combined attach should continue ordered updates"
        );
    }

    run_case(false);
    run_case(true);
}

#[test]
fn aud0_dma_first_word_arrival_delay_increases_with_sprite_dma_contention() {
    fn first_word_arrival_cck(enable_sprite_dma: bool) -> u32 {
        let mut amiga = make_test_amiga();
        let sample_addr = 0x0000_3200u32;
        amiga.memory.write_byte(sample_addr, 0x7F);
        amiga.memory.write_byte(sample_addr + 1, 0x80);

        configure_aud0_dma(&mut amiga, sample_addr, 1, 124, 64);
        let mut dmacon = 0x8000 | DMACON_DMAEN | DMACON_AUD0EN;
        if enable_sprite_dma {
            dmacon |= DMACON_SPREN;
        }
        amiga.write_custom_reg(REG_DMACON, dmacon);

        tick_until(&mut amiga, 2_000, |a| {
            a.paula.read_audio_register(REG_AUD0DAT) == Some(0x7F80)
        })
        .expect("AUD0 DMA word should arrive")
    }

    let baseline = first_word_arrival_cck(false);
    let contended = first_word_arrival_cck(true);
    assert!(
        contended > baseline,
        "sprite DMA contention should delay Paula DMA word return \
         (baseline={baseline}, contended={contended})"
    );
}

#[test]
fn aud0_dma_first_word_arrival_delay_increases_with_busy_copper_vs_waiting_copper() {
    fn write_chip_word(amiga: &mut Amiga, addr: u32, word: u16) {
        amiga.memory.write_byte(addr, (word >> 8) as u8);
        amiga.memory.write_byte(addr + 1, word as u8);
    }

    fn first_word_arrival_cck(copper_busy: bool) -> u32 {
        let mut amiga = make_test_amiga();
        let sample_addr = 0x0000_3400u32;
        let copper_addr = 0x0000_3800u32;

        amiga.memory.write_byte(sample_addr, 0x7F);
        amiga.memory.write_byte(sample_addr + 1, 0x80);
        configure_aud0_dma(&mut amiga, sample_addr, 1, 124, 64);

        if copper_busy {
            // Repeated MOVE COLOR00,<val> keeps the copper in Fetch1/Fetch2,
            // causing real chip-memory reads on copper slots.
            for i in 0..16u32 {
                let base = copper_addr + i * 4;
                write_chip_word(&mut amiga, base, 0x0180);
                write_chip_word(&mut amiga, base + 2, i as u16);
            }
        } else {
            // End-of-list marker -> copper fetches the pair, then sits in WAIT
            // without further chip-memory reads.
            write_chip_word(&mut amiga, copper_addr, 0xFFFF);
            write_chip_word(&mut amiga, copper_addr + 2, 0xFFFE);
        }
        write_cop1_lc(&mut amiga, copper_addr);

        amiga.write_custom_reg(
            REG_DMACON,
            0x8000 | DMACON_DMAEN | DMACON_AUD0EN | DMACON_SPREN | DMACON_COPEN,
        );

        tick_until(&mut amiga, 4_000, |a| {
            a.paula.read_audio_register(REG_AUD0DAT) == Some(0x7F80)
        })
        .expect("AUD0 DMA word should arrive")
    }

    let waiting = first_word_arrival_cck(false);
    let busy = first_word_arrival_cck(true);

    assert!(
        busy > waiting,
        "busy copper fetches should delay Paula DMA word return more than copper WAIT \
         (waiting={waiting}, busy={busy})"
    );
}

#[test]
fn aud0dat_write_is_ignored_for_playback_when_dma_enabled_before_first_tick() {
    let mut amiga = make_test_amiga();
    let sample_addr = 0x0000_2800u32;

    // DMA sample word is positive. CPU tries to inject a strongly negative word
    // after enabling DMA but before Paula has synchronized the channel enable.
    amiga.memory.write_byte(sample_addr, 0x40);
    amiga.memory.write_byte(sample_addr + 1, 0x40);
    configure_aud0_dma(&mut amiga, sample_addr, 1, 124, 64);
    amiga.write_custom_reg(REG_DMACON, 0x8000 | DMACON_DMAEN | DMACON_AUD0EN);
    amiga.write_custom_reg(REG_AUD0DAT, 0xC0C0);

    tick_ccks(&mut amiga, 124);
    let (left, right) = amiga.paula.mix_audio_stereo();

    assert!(
        left > 0.2,
        "AUD0DAT CPU write should not override DMA-owned playback before first tick (left={left})"
    );
    assert!(
        right.abs() < 0.01,
        "AUD0 should not drive right (right={right})"
    );
}

#[test]
fn aud0dat_write_does_not_override_active_dma_playback() {
    let mut amiga = make_test_amiga();
    let sample_addr = 0x0000_2A00u32;

    amiga.memory.write_byte(sample_addr, 0x40);
    amiga.memory.write_byte(sample_addr + 1, 0x40);
    configure_aud0_dma(&mut amiga, sample_addr, 1, 124, 64);
    amiga.write_custom_reg(REG_DMACON, 0x8000 | DMACON_DMAEN | DMACON_AUD0EN);

    // Let DMA start and produce an audible sample.
    tick_ccks(&mut amiga, 124);
    let (left_before, right_before) = amiga.paula.mix_audio_stereo();
    assert!(left_before > 0.2, "expected DMA-driven AUD0 output");
    assert!(right_before.abs() < 0.01);

    amiga.write_custom_reg(REG_AUD0DAT, 0xC0C0);
    tick_ccks(&mut amiga, 124);
    let (left_after, right_after) = amiga.paula.mix_audio_stereo();

    assert!(
        left_after > 0.2,
        "AUD0DAT CPU write should not override active DMA playback (left={left_after})"
    );
    assert!(right_after.abs() < 0.01);
}

const REG_AUD1LCH: u16 = 0x0B0;
const REG_AUD1LCL: u16 = 0x0B2;
const REG_AUD1LEN: u16 = 0x0B4;
const REG_AUD1PER: u16 = 0x0B6;
const REG_AUD1VOL: u16 = 0x0B8;

fn save_stereo_wav(samples: &[f32], path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for &sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        let scaled = (clamped * f32::from(i16::MAX)) as i16;
        writer.write_sample(scaled)?;
    }
    writer.finalize()?;
    Ok(())
}

#[test]
#[ignore]
fn test_paula_audio_dma_capture() {
    let mut amiga = make_test_amiga();

    // Write a 32-byte square wave into chip RAM at $2000.
    // First 16 bytes = +127, next 16 bytes = -128.
    let sample_addr = 0x0000_2000u32;
    for i in 0..16u32 {
        amiga.memory.write_byte(sample_addr + i, 0x7F);
    }
    for i in 16..32u32 {
        amiga.memory.write_byte(sample_addr + i, 0x80);
    }

    // Configure AUD0 (left channel): pointer, length, period, volume.
    // Period 252 CCK with 32-byte waveform → 3,546,895 / (32 * 252) ≈ 440 Hz.
    amiga.write_custom_reg(REG_AUD0LCH, (sample_addr >> 16) as u16);
    amiga.write_custom_reg(REG_AUD0LCL, (sample_addr & 0xFFFF) as u16);
    amiga.write_custom_reg(REG_AUD0LEN, 16); // 16 words = 32 bytes
    amiga.write_custom_reg(REG_AUD0PER, 252);
    amiga.write_custom_reg(REG_AUD0VOL, 64);

    // Configure AUD1 (right channel) with the same waveform.
    amiga.write_custom_reg(REG_AUD1LCH, (sample_addr >> 16) as u16);
    amiga.write_custom_reg(REG_AUD1LCL, (sample_addr & 0xFFFF) as u16);
    amiga.write_custom_reg(REG_AUD1LEN, 16);
    amiga.write_custom_reg(REG_AUD1PER, 252);
    amiga.write_custom_reg(REG_AUD1VOL, 64);

    // Enable DMA for both audio channels.
    amiga.write_custom_reg(
        REG_DMACON,
        0x8000 | DMACON_DMAEN | DMACON_AUD0EN | DMACON_AUD1EN,
    );

    // Run 50 PAL frames (~1 second) and accumulate stereo audio.
    let mut all_audio: Vec<f32> = Vec::new();
    for _ in 0..50 {
        amiga.run_frame();
        all_audio.extend_from_slice(&amiga.take_audio_buffer());
    }

    // Stereo frames = total samples / 2 (interleaved L, R).
    let stereo_frames = all_audio.len() / 2;
    assert!(
        all_audio.len() % 2 == 0,
        "audio buffer should contain interleaved stereo pairs"
    );
    assert!(
        stereo_frames > 40_000,
        "expected ~48,000 stereo frames for 1 second at 48 kHz, got {stereo_frames}"
    );

    // Check both channels are non-silent.
    let mut left_max = 0.0f32;
    let mut right_max = 0.0f32;
    for frame in all_audio.chunks_exact(2) {
        left_max = left_max.max(frame[0].abs());
        right_max = right_max.max(frame[1].abs());
    }
    assert!(
        left_max > 0.1,
        "left channel should be non-silent (max abs = {left_max})"
    );
    assert!(
        right_max > 0.1,
        "right channel should be non-silent (max abs = {right_max})"
    );

    // Save as stereo WAV.
    let out_dir = Path::new("../../test_output");
    fs::create_dir_all(out_dir).ok();
    let wav_path = out_dir.join("amiga_paula_tone.wav");
    save_stereo_wav(&all_audio, &wav_path).expect("failed to save stereo WAV");
    assert!(wav_path.exists());

    eprintln!(
        "Saved Paula audio to {} ({} stereo frames, L max={left_max:.3}, R max={right_max:.3})",
        wav_path.display(),
        stereo_frames,
    );
}
