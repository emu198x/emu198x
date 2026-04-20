//! Phase 1 characterization tests — Paula audio channels.
//!
//! Per HRM chapter 5 (Audio Hardware):
//!
//!  - Four identical DMA-driven channels (0-3), each with:
//!      AUDxLC  ($0A0/0B0/0C0/0D0)  20-bit chip RAM base pointer
//!      AUDxLEN ($0A4/0B4/0C4/0D4)  length in 16-bit words (0 = 65536)
//!      AUDxPER ($0A6/0B6/0C6/0D6)  period in colour clocks per sample
//!      AUDxVOL ($0A8/0B8/0C8/0D8)  0..=64 volume
//!      AUDxDAT ($0AA/0BA/0CA/0DA)  data register for non-DMA playback
//!  - Stereo routing: channels 0 + 3 → left, channels 1 + 2 → right.
//!  - Minimum PER for real-hardware playback is 124 CCK (below this
//!    the DMA slot cannot keep up); AUDxPER writes below 124 are
//!    preserved for read-back but clamped for actual timing.
//!  - DMA enable rising edge starts the channel: load LC → words=LEN,
//!    fetch first word, raise AUDx IRQ bit at the start of the block.
//!  - Block end (words_remaining = 0) reloads from LC and raises the
//!    AUDx IRQ again.
//!  - ADKCON bits 0-3 (USE_VOLUME) enable channel N to modulate the
//!    volume of channel N+1 (channel 3 wraps off the end — ignored).
//!    ADKCON bits 4-7 (USE_PERIOD) do the same for period.
//!    A modulator channel's own output is muted.

use commodore_paula_8364::Paula8364;

const DMACON_MASTER: u16 = 0x0200;
const DMACON_AUD0: u16 = 0x0001;
const DMACON_AUD1: u16 = 0x0002;
const DMACON_AUD3: u16 = 0x0008;
const AUDIO_MIN_PERIOD_CCK: u16 = 124;

const AUD0_LC_HI: u16 = 0x0A0;
const AUD0_LC_LO: u16 = 0x0A2;
const AUD0_LEN: u16 = 0x0A4;
const AUD0_PER: u16 = 0x0A6;
const AUD0_VOL: u16 = 0x0A8;

const AUD1_LC_HI: u16 = 0x0B0;
const AUD1_LC_LO: u16 = 0x0B2;
const AUD1_LEN: u16 = 0x0B4;
const AUD1_VOL: u16 = 0x0B8;

// Channel 2 base = $0C0, channel 3 base = $0D0.
const AUD3_LC_HI: u16 = 0x0D0;
const AUD3_LC_LO: u16 = 0x0D2;
const AUD3_LEN: u16 = 0x0D4;

fn zero_reader(_: u32) -> u8 { 0 }

// ────────────────────────────────────────────────────────────────
// Register storage
// ────────────────────────────────────────────────────────────────

#[test]
fn audio_registers_round_trip_per_channel() {
    let mut paula = Paula8364::new();
    assert!(paula.write_audio_register(AUD0_LC_HI, 0x0012));
    assert!(paula.write_audio_register(AUD0_LC_LO, 0x3456));
    assert!(paula.write_audio_register(AUD0_LEN, 0x0008));
    assert!(paula.write_audio_register(AUD0_PER, 500));
    assert!(paula.write_audio_register(AUD0_VOL, 32));
    assert_eq!(paula.read_audio_register(AUD0_LC_HI), Some(0x0012));
    assert_eq!(paula.read_audio_register(AUD0_LC_LO), Some(0x3456));
    assert_eq!(paula.read_audio_register(AUD0_LEN), Some(0x0008));
    assert_eq!(paula.read_audio_register(AUD0_PER), Some(500));
    assert_eq!(paula.read_audio_register(AUD0_VOL), Some(32));
}

#[test]
fn audio_lc_low_word_masks_off_bit_0_for_word_alignment() {
    // Chip RAM is word-addressed; bit 0 of the low half of LC must
    // not be settable (HRM: "must be even").
    let mut paula = Paula8364::new();
    paula.write_audio_register(AUD0_LC_LO, 0xFFFF);
    assert_eq!(paula.read_audio_register(AUD0_LC_LO), Some(0xFFFE));
}

#[test]
fn audio_vol_is_clamped_to_64_at_write_time() {
    let mut paula = Paula8364::new();
    paula.write_audio_register(AUD0_VOL, 0x007F);
    assert_eq!(paula.read_audio_register(AUD0_VOL), Some(64),
        "HRM: volume > 64 is meaningless; chip clamps");
    paula.write_audio_register(AUD0_VOL, 0x0800); // high bits ignored
    assert_eq!(paula.read_audio_register(AUD0_VOL), Some(0));
}

#[test]
fn audio_register_writes_outside_channel_range_are_rejected() {
    let mut paula = Paula8364::new();
    // $09E is ADKCON, not an audio register.
    assert!(!paula.write_audio_register(0x09E, 0));
    assert_eq!(paula.read_audio_register(0x09E), None);
}

// ────────────────────────────────────────────────────────────────
// AUDxPER minimum period clamp
// ────────────────────────────────────────────────────────────────

#[test]
fn audx_per_below_minimum_readback_preserves_written_value() {
    let mut paula = Paula8364::new();
    paula.write_audio_register(AUD0_PER, 1);
    assert_eq!(paula.read_audio_register(AUD0_PER), Some(1),
        "readback preserves written value, even below playback minimum");
}

#[test]
fn audx_per_below_minimum_uses_clamp_for_playback_timing() {
    // Program a channel with PER = 1 and DMA enabled. If PER were
    // used literally, samples would produce visible mixer output in
    // the first few CCK. Clamped to 124, no output for 123 CCK.
    let mut paula = Paula8364::new();
    let dmacon = DMACON_MASTER | DMACON_AUD0;
    paula.write_audio_register(AUD0_LC_HI, 0x0000);
    paula.write_audio_register(AUD0_LC_LO, 0x1000);
    paula.write_audio_register(AUD0_LEN, 0x0001);
    paula.write_audio_register(AUD0_PER, 1);
    paula.write_audio_register(AUD0_VOL, 64);

    let sample = |_: u32| 0x7F; // max positive sample

    for _ in 0..(AUDIO_MIN_PERIOD_CCK as usize - 1) {
        paula.tick_audio_cck(dmacon, Some(0), sample);
    }
    let (left, _) = paula.mix_audio_stereo();
    assert!(left.abs() < 0.01,
        "no playback output before the minimum period elapses; got {left}");

    paula.tick_audio_cck(dmacon, Some(0), sample);
    let (left, _) = paula.mix_audio_stereo();
    assert!(left > 0.4,
        "playback output appears after clamped-minimum period; got {left}");
}

// ────────────────────────────────────────────────────────────────
// DMA enable → block-start IRQ
// ────────────────────────────────────────────────────────────────

#[test]
fn dma_enable_rising_edge_raises_audx_irq() {
    let mut paula = Paula8364::new();
    paula.write_audio_register(AUD0_LC_HI, 0);
    paula.write_audio_register(AUD0_LC_LO, 0x1000);
    paula.write_audio_register(AUD0_LEN, 0x0010);
    paula.write_audio_register(AUD0_PER, 500);

    let dmacon = DMACON_MASTER | DMACON_AUD0;
    paula.tick_audio_cck(dmacon, Some(0), zero_reader);
    // INTREQ bit 7 = AUD0.
    assert_ne!(paula.intreq & 0x0080, 0, "AUD0 IRQ should fire on DMA start");
    assert_eq!(paula.intreq & 0x0700, 0,
        "other audio channels should not have fired");
}

#[test]
fn channel_3_maps_to_intreq_bit_10() {
    let mut paula = Paula8364::new();
    paula.write_audio_register(AUD3_LC_HI, 0);
    paula.write_audio_register(AUD3_LC_LO, 0x1000);
    paula.write_audio_register(AUD3_LEN, 0x0010);

    let dmacon = DMACON_MASTER | DMACON_AUD3;
    paula.tick_audio_cck(dmacon, Some(3), zero_reader);
    assert_ne!(paula.intreq & 0x0400, 0, "AUD3 IRQ = INTREQ bit 10");
}

// ────────────────────────────────────────────────────────────────
// Stereo routing per HRM
// ────────────────────────────────────────────────────────────────

#[test]
fn channels_0_and_3_route_to_left_and_1_2_to_right() {
    let mut paula = Paula8364::new();

    // Feed +max on ch 1 (right); ch 0 (left) stays silent.
    paula.write_audio_register(AUD1_LC_HI, 0);
    paula.write_audio_register(AUD1_LC_LO, 0x2000);
    paula.write_audio_register(AUD1_LEN, 1);
    paula.write_audio_register(AUD1_VOL, 64);

    let read = |_: u32| 0x7F;
    let dmacon = DMACON_MASTER | DMACON_AUD1;
    for _ in 0..AUDIO_MIN_PERIOD_CCK {
        paula.tick_audio_cck(dmacon, Some(1), read);
    }
    let (left, right) = paula.mix_audio_stereo();
    assert!(left.abs() < 0.01, "left silent when only ch 1 active, got {left}");
    assert!(right > 0.4, "right has ch 1 output, got {right}");
}

// ────────────────────────────────────────────────────────────────
// ADKCON attach modulation
// ────────────────────────────────────────────────────────────────

#[test]
fn attach_period_bit_mutes_modulator_channel_in_stereo_mix() {
    let mut paula = Paula8364::new();
    // Channel 0 modulates channel 1's period via ADKCON bit 4.
    paula.write_adkcon(0x8010);

    // Ch 0: max output, would normally show up on the LEFT.
    paula.write_audio_register(AUD0_LC_HI, 0);
    paula.write_audio_register(AUD0_LC_LO, 0x1000);
    paula.write_audio_register(AUD0_LEN, 1);
    paula.write_audio_register(AUD0_VOL, 64);

    let read = |_: u32| 0x7F;
    let dmacon = DMACON_MASTER | DMACON_AUD0;
    for _ in 0..AUDIO_MIN_PERIOD_CCK {
        paula.tick_audio_cck(dmacon, Some(0), read);
    }
    let (left, _) = paula.mix_audio_stereo();
    assert!(left.abs() < 0.01,
        "modulator channels must not contribute to the audio mix; got {left}");
}

#[test]
fn attach_volume_uses_channel_n_low_byte_to_set_volume_on_n_plus_1() {
    // HRM: when channel N has USE_VOLUME set, its low-byte transition
    // writes AUDx+1_VOL. Confirm the volume write is observable via
    // the register read-back once modulation has fired.
    let mut paula = Paula8364::new();
    paula.write_adkcon(0x8001); // channel 0 modulates ch 1 volume

    // Ch 0 will output samples with known high/low bytes.
    paula.write_audio_register(AUD0_LC_HI, 0);
    paula.write_audio_register(AUD0_LC_LO, 0x1000);
    paula.write_audio_register(AUD0_LEN, 1);
    paula.write_audio_register(AUD0_VOL, 64);

    // Read function returns $7F on HI, $20 on LO so ch 1 vol → 0x20.
    let read = |addr: u32| if addr & 1 == 0 { 0x7F } else { 0x20 };
    let dmacon = DMACON_MASTER | DMACON_AUD0;
    for _ in 0..(AUDIO_MIN_PERIOD_CCK * 2 + 4) {
        paula.tick_audio_cck(dmacon, Some(0), read);
    }
    let ch1_vol = paula.read_audio_register(0x0B8).unwrap();
    assert_eq!(ch1_vol, 0x20,
        "channel 1 volume should have been written by channel 0's low-byte event; got {ch1_vol}");
}

// ────────────────────────────────────────────────────────────────
// Reset
// ────────────────────────────────────────────────────────────────

#[test]
fn reset_clears_all_audio_registers_and_irq() {
    let mut paula = Paula8364::new();
    paula.write_audio_register(AUD0_VOL, 64);
    paula.write_audio_register(AUD0_PER, 999);
    paula.write_intreq(0x8080); // AUD0 pending

    paula.reset();

    assert_eq!(paula.read_audio_register(AUD0_VOL), Some(0));
    // Archive default PER is MIN_AUDIO_PERIOD_CCK (124) — the minimum
    // real hardware can actually play back. Readback preserves this.
    assert_eq!(paula.read_audio_register(AUD0_PER), Some(124));
    assert_eq!(paula.intreq, 0);
}
