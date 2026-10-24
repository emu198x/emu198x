use super::*;

const MSX_CLOCK: f64 = 3_579_545.0;

/// A ramp from -128 to 120 in steps of 8.
fn ramp() -> [u8; WAVE_LENGTH] {
    std::array::from_fn(|i| (i as u8 * 8) ^ 0x80)
}

fn load_wave(scc: &mut Scc, channel: u8, wave: &[u8; WAVE_LENGTH]) {
    for (i, &value) in wave.iter().enumerate() {
        scc.write(channel * 32 + i as u8, value);
    }
}

/// Set a channel's period through the SCC's frequency block.
fn set_period(scc: &mut Scc, base: u8, channel: u8, period: u16) {
    scc.write(base + channel * 2, period as u8);
    scc.write(base + channel * 2 + 1, (period >> 8) as u8);
}

/// The channel's output once per sample step, for `steps` steps.
fn play(scc: &mut Scc, channel: usize, period: u16, steps: usize) -> Vec<i16> {
    (0..steps)
        .map(|_| {
            for _ in 0..=period {
                scc.tick();
            }
            scc.channel_output(channel)
        })
        .collect()
}

#[test]
fn power_on_state() {
    let scc = Scc::new(Variant::Scc);
    assert_eq!(scc.mode(), Mode::Scc);
    assert_eq!(scc.peek(0x00), 0xFF, "waveforms power up as $FF");
    assert_eq!(scc.volume(4), 15);
    assert_eq!(scc.enable_bits(), 0);
    assert_eq!(scc.deform(), 0);
    assert_eq!(scc.period(0), 0);
    assert_eq!(scc.frequency_hz(0, MSX_CLOCK), 0.0, "a stopped channel");
    assert_eq!(scc.output(), 0);
    assert_eq!(Scc::new(Variant::SccI).mode(), Mode::Compatible);
}

#[test]
fn a_channel_plays_its_waveform_scaled_by_volume() {
    let mut scc = Scc::new(Variant::Scc);
    let wave = ramp();
    load_wave(&mut scc, 0, &wave);
    scc.write(0x8A, 15);
    scc.write(0x8F, 0x01);
    set_period(&mut scc, 0x80, 0, 100);
    let played = play(&mut scc, 0, 100, 32);
    // The first step moves to sample 1; after 32 steps it is back at 0.
    let expected: Vec<i16> = (1..=32)
        .map(|i| (i16::from(wave[i % 32] as i8) * 15) >> 4)
        .collect();
    assert_eq!(played, expected);
    assert_eq!(scc.output(), i32::from(expected[31]));
}

#[test]
fn the_frequency_is_clock_over_32_periods_plus_one() {
    let mut scc = Scc::new(Variant::Scc);
    set_period(&mut scc, 0x80, 2, 253);
    assert_eq!(scc.period(2), 253);
    let hz = scc.frequency_hz(2, MSX_CLOCK);
    assert!((hz - 440.0).abs() < 1.0, "period 253 is A440; got {hz}");

    // Count the steps over a second of clocks.
    let mut wave = [0u8; WAVE_LENGTH];
    wave[0] = 100;
    load_wave(&mut scc, 2, &wave);
    scc.write(0x8F, 0x04);
    let mut cycles = 0;
    let mut was_high = false;
    for _ in 0..3_579_545 {
        scc.tick();
        let high = scc.channel_output(2) > 0;
        if high && !was_high {
            cycles += 1;
        }
        was_high = high;
    }
    assert!((439..=441).contains(&cycles), "{cycles} cycles in a second");
}

#[test]
fn volume_takes_effect_at_the_next_sample() {
    let mut scc = Scc::new(Variant::Scc);
    load_wave(&mut scc, 1, &[64; WAVE_LENGTH]);
    scc.write(0x8F, 0x02);
    set_period(&mut scc, 0x80, 1, 20);
    assert_eq!(
        scc.channel_output(1),
        60,
        "the frequency write shows the sample"
    );
    scc.write(0x8B, 8);
    assert_eq!(scc.channel_output(1), 60, "the new volume waits for a step");
    play(&mut scc, 1, 20, 1);
    assert_eq!(scc.channel_output(1), 32);
}

#[test]
fn the_low_four_bits_are_dropped_per_channel() {
    let mut scc = Scc::new(Variant::Scc);
    // +1 at volume 15 is 15 >> 4 = 0; -1 is -15 >> 4 = -1.
    load_wave(&mut scc, 0, &[1; WAVE_LENGTH]);
    load_wave(&mut scc, 1, &[0xFF; WAVE_LENGTH]);
    scc.write(0x8F, 0x03);
    set_period(&mut scc, 0x80, 0, 50);
    set_period(&mut scc, 0x80, 1, 50);
    assert_eq!((scc.channel_output(0), scc.channel_output(1)), (0, -1));
    assert_eq!(scc.output(), -1);
}

#[test]
fn a_short_period_stops_the_channel() {
    let mut scc = Scc::new(Variant::Scc);
    load_wave(&mut scc, 0, &ramp());
    scc.write(0x8F, 0x01);
    set_period(&mut scc, 0x80, 0, 8);
    let first = scc.channel_output(0);
    for _ in 0..1000 {
        scc.tick();
    }
    assert_eq!(scc.channel_output(0), first, "frozen on its sample");
}

#[test]
fn a_disabled_channel_is_silent_until_its_next_step() {
    let mut scc = Scc::new(Variant::Scc);
    load_wave(&mut scc, 0, &[50; WAVE_LENGTH]);
    scc.write(0x8F, 0x01);
    set_period(&mut scc, 0x80, 0, 30);
    scc.write(0x8F, 0x00);
    scc.tick();
    assert_eq!(scc.channel_output(0), 0);
    scc.write(0x8F, 0x01);
    assert_eq!(scc.channel_output(0), 0, "silent after re-enabling");
    play(&mut scc, 0, 30, 1);
    assert_eq!(scc.channel_output(0), 46);
}

#[test]
fn channels_4_and_5_share_a_waveform_on_the_scc() {
    let mut scc = Scc::new(Variant::Scc);
    scc.write(0x60, 0x40);
    assert_eq!(scc.peek(0x60), 0x40);
    scc.write(0x8F, 0x10);
    set_period(&mut scc, 0x80, 4, 40);
    assert_eq!(
        scc.channel_output(4),
        60,
        "channel 5 plays channel 4's wave"
    );
    // $80-$FF read as $FF on the SCC.
    assert_eq!(scc.peek(0x80), 0xFF);
    assert_eq!(scc.peek(0xA0), 0xFF);
}

#[test]
fn the_frequency_block_appears_twice() {
    let mut scc = Scc::new(Variant::Scc);
    scc.write(0x90, 0x34);
    scc.write(0x91, 0x12);
    assert_eq!(scc.period(0), 0x234, "the high byte keeps four bits");
    scc.write(0x9A, 0x37);
    assert_eq!(scc.volume(0), 7);
    scc.write(0x9F, 0x1F);
    assert_eq!(scc.enable_bits(), 0x1F);
}

#[test]
fn deformation_narrows_the_period() {
    let mut scc = Scc::new(Variant::Scc);
    scc.write(0xE0, 0x02);
    set_period(&mut scc, 0x80, 0, 0x345);
    assert_eq!(scc.period(0), 0x45, "8-bit periods");
    scc.write(0xE0, 0x01);
    set_period(&mut scc, 0x80, 0, 0x345);
    assert_eq!(scc.period(0), 0x3, "4-bit periods");
    scc.write(0xE0, 0x03);
    set_period(&mut scc, 0x80, 0, 0x345);
    assert_eq!(scc.period(0), 0x45, "bit 1 wins");
}

#[test]
fn deformation_bit_5_restarts_the_waveform() {
    let mut scc = Scc::new(Variant::Scc);
    let wave = ramp();
    load_wave(&mut scc, 0, &wave);
    scc.write(0x8F, 0x01);
    set_period(&mut scc, 0x80, 0, 20);
    play(&mut scc, 0, 20, 5);
    set_period(&mut scc, 0x80, 0, 20);
    assert_ne!(scc.channel_output(0), (i16::from(wave[0] as i8) * 15) >> 4);
    scc.write(0xE0, 0x20);
    set_period(&mut scc, 0x80, 0, 20);
    assert_eq!(scc.channel_output(0), (i16::from(wave[0] as i8) * 15) >> 4);
}

#[test]
fn rotation_turns_reads_and_blocks_writes() {
    let mut scc = Scc::new(Variant::Scc);
    let wave = ramp();
    load_wave(&mut scc, 0, &wave);
    set_period(&mut scc, 0x80, 0, 9);
    scc.write(0xE0, 0x40);
    for _ in 0..30 {
        scc.tick();
    }
    assert_eq!(scc.peek(0x00), wave[3], "turned three samples");
    scc.write(0x00, 0x11);
    assert_eq!(scc.peek(0x00), wave[3], "read-only while rotating");
    scc.write(0xE0, 0x00);
    scc.write(0x00, 0x11);
    assert_eq!(scc.peek(0x00), 0x11);
}

#[test]
fn bit_7_rotates_channels_4_and_5_on_the_scc_only() {
    let mut scc = Scc::new(Variant::Scc);
    scc.write(0xE0, 0x80);
    scc.write(0x00, 0x22);
    assert_eq!(scc.peek(0x00), 0x22, "channel 1 stays writable");
    scc.write(0x60, 0x22);
    assert_eq!(scc.peek(0x60), 0xFF, "channel 4 is read-only");

    scc.write(0xE0, 0xC0);
    scc.write(0x01, 0x33);
    assert_eq!(scc.peek(0x01), 0xFF, "every waveform is read-only");

    let mut scc_i = Scc::new(Variant::SccI);
    scc_i.write(0xC0, 0x80);
    scc_i.write(0x60, 0x22);
    assert_eq!(scc_i.peek(0x60), 0x22, "the SCC-I has no bit 7");
}

#[test]
fn channels_4_and_5_rotate_at_channel_5s_rate() {
    let mut scc = Scc::new(Variant::Scc);
    let wave = ramp();
    load_wave(&mut scc, 3, &wave);
    set_period(&mut scc, 0x80, 3, 100);
    set_period(&mut scc, 0x80, 4, 9);
    scc.write(0xE0, 0x40);
    for _ in 0..20 {
        scc.tick();
    }
    assert_eq!(scc.peek(0x60), wave[2]);
}

#[test]
fn reading_the_deformation_register_writes_ff() {
    let mut scc = Scc::new(Variant::Scc);
    assert_eq!(scc.read(0xE5), 0xFF);
    assert_eq!(scc.deform(), 0xFF);
    assert_eq!(scc.peek(0x00), 0xFF);
    let mut scc_i = Scc::new(Variant::SccI);
    scc_i.read(0x10);
    assert_eq!(scc_i.deform(), 0, "an ordinary read has no effect");
    scc_i.read(0xC3);
    assert_eq!(scc_i.deform(), 0xFF);
}

#[test]
fn scc_i_compatible_mode_keeps_the_scc_map() {
    let mut scc = Scc::new(Variant::SccI);
    scc.write(0x60, 0x40);
    assert_eq!(scc.peek(0xA0), 0x40, "channel 5's wave reads at $A0");
    scc.write(0xA0, 0x10);
    assert_eq!(scc.peek(0xA0), 0x40, "and cannot be written there");
    scc.write(0x8F, 0x1F);
    assert_eq!(scc.enable_bits(), 0x1F);
    scc.write(0xC0, 0x02);
    assert_eq!(scc.deform(), 0x02);
    scc.write(0xE0, 0x00);
    assert_eq!(scc.deform(), 0x02, "$E0 is nothing in compatible mode");
}

#[test]
fn scc_plus_mode_gives_channel_5_its_own_waveform() {
    let mut scc = Scc::new(Variant::SccI);
    scc.set_plus_mode(true);
    assert_eq!(scc.mode(), Mode::Plus);
    scc.write(0x60, 0x40);
    scc.write(0x80, 0x20);
    assert_eq!((scc.peek(0x60), scc.peek(0x80)), (0x40, 0x20));
    scc.write(0xAF, 0x18);
    set_period(&mut scc, 0xA0, 3, 40);
    set_period(&mut scc, 0xA0, 4, 40);
    assert_eq!((scc.channel_output(3), scc.channel_output(4)), (60, 30));
    assert_eq!(scc.peek(0xA0), 0xFF, "the frequency block is write-only");
    scc.write(0xC0, 0x01);
    assert_eq!(scc.deform(), 0x01);

    scc.set_plus_mode(false);
    assert_eq!(scc.mode(), Mode::Compatible);
    let mut plain = Scc::new(Variant::Scc);
    plain.set_plus_mode(true);
    assert_eq!(plain.mode(), Mode::Scc, "the SCC has one mode");
}

#[test]
fn reset_clears_enables_and_deformation() {
    let mut scc = Scc::new(Variant::SccI);
    scc.set_plus_mode(true);
    scc.write(0xAF, 0x1F);
    scc.write(0xC0, 0x40);
    scc.write(0xAA, 3);
    scc.reset();
    assert_eq!(scc.mode(), Mode::Compatible);
    assert_eq!((scc.enable_bits(), scc.deform()), (0, 0));
    assert_eq!(scc.volume(0), 3, "volumes survive a reset");
    let mut plain = Scc::new(Variant::Scc);
    plain.reset();
    assert_eq!(plain.mode(), Mode::Scc);
}
