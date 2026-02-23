use machine_amiga::memory::ROM_BASE;
use machine_amiga::{Amiga, TICKS_PER_CCK};

const REG_DMACON: u16 = 0x096;
const REG_AUD0LCH: u16 = 0x0A0;
const REG_AUD0LCL: u16 = 0x0A2;
const REG_AUD0LEN: u16 = 0x0A4;
const REG_AUD0PER: u16 = 0x0A6;
const REG_AUD0VOL: u16 = 0x0A8;
const REG_AUD0DAT: u16 = 0x0AA;
const AUD_STRIDE: u16 = 0x10;
const AUD_REG_PER_OFFSET: u16 = 0x06;
const AUD_REG_VOL_OFFSET: u16 = 0x08;
const AUD_REG_DAT_OFFSET: u16 = 0x0A;

const DMACON_DMAEN: u16 = 0x0200;
const DMACON_AUD0EN: u16 = 0x0001;
const INTREQ_AUD0: u16 = 0x0080;

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
