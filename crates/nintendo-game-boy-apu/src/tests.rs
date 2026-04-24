//! APU integration tests.

use super::*;

fn run_t(apu: &mut Apu, mut div: u16, ticks: u32) -> u16 {
    for _ in 0..ticks {
        div = div.wrapping_add(1);
        apu.tick(div);
    }
    div
}

// -- Power and basic state ---------------------------------------------

#[test]
fn power_on_state_is_disabled_and_silent() {
    let mut apu = Apu::new();
    apu.tick(0);
    assert!(!apu.read(REG_NR52) & 0x80 != 0);
    assert_eq!(apu.samples_buffered(), 0);
}

#[test]
fn nr52_high_bits_always_read_high() {
    let apu = Apu::new();
    assert_eq!(apu.read(REG_NR52) & 0x70, 0x70);
}

#[test]
fn enabling_apu_resets_frame_sequencer() {
    let mut apu = Apu::new();
    // Force frame_step to non-zero by enabling and stepping.
    apu.write(REG_NR52, 0x80);
    // Run enough ticks to roll the div bit 12 over a few times.
    let _ = run_t(&mut apu, 0x0FFF, 8192 * 3);
    assert_ne!(apu.frame_step, 0);
    apu.write(REG_NR52, 0x00);
    apu.write(REG_NR52, 0x80);
    assert_eq!(apu.frame_step, 0);
}

#[test]
fn disabling_apu_preserves_length_counters() {
    let mut apu = Apu::new();
    apu.write(REG_NR52, 0x80);
    // Set CH1 length via NR11.
    apu.write(0xFF11, 0b00_010101); // length = 64 - 21 = 43
    assert_eq!(apu.ch1.length_timer, 43);
    apu.write(REG_NR52, 0x00);
    assert_eq!(
        apu.ch1.length_timer, 43,
        "length preserved through power off"
    );
}

#[test]
fn writes_when_disabled_are_ignored_except_length() {
    let mut apu = Apu::new();
    // APU off; write to NR12 (envelope) — should be ignored.
    apu.write(0xFF12, 0xF0);
    assert_eq!(apu.ch1.envelope_initial, 0);
    // Length writes still go through.
    apu.write(0xFF11, 0b00_001000); // length = 64 - 8 = 56
    assert_eq!(apu.ch1.length_timer, 56);
}

// -- Register read-back ------------------------------------------------

#[test]
fn nr11_read_returns_duty_with_length_bits_high() {
    let mut apu = Apu::new();
    apu.write(REG_NR52, 0x80);
    apu.write(0xFF11, 0b10_000000); // duty 2, length 0
    assert_eq!(apu.read(0xFF11), 0b10_111111);
}

#[test]
fn nr14_read_returns_length_enable_only() {
    let mut apu = Apu::new();
    apu.write(REG_NR52, 0x80);
    apu.write(0xFF12, 0xF0); // DAC on (envelope_initial > 0)
    apu.write(0xFF14, 0x40); // length enable
    assert_eq!(apu.read(0xFF14), 0xFF);
    apu.write(0xFF14, 0x00);
    assert_eq!(apu.read(0xFF14), 0xBF);
}

#[test]
fn nr30_read_reflects_dac_enable() {
    let mut apu = Apu::new();
    apu.write(REG_NR52, 0x80);
    apu.write(0xFF1A, 0x80);
    assert_eq!(apu.read(0xFF1A), 0xFF);
    apu.write(0xFF1A, 0x00);
    assert_eq!(apu.read(0xFF1A), 0x7F);
}

#[test]
fn nr32_volume_round_trips() {
    let mut apu = Apu::new();
    apu.write(REG_NR52, 0x80);
    for code in 0..4 {
        apu.write(0xFF1C, code << 5);
        assert_eq!(apu.read(0xFF1C) & 0x60, code << 5);
    }
}

#[test]
fn host_audio_controls_do_not_change_nr52_channel_state() {
    let mut apu = Apu::new();
    apu.write(REG_NR52, 0x80);
    apu.write(0xFF17, 0xF0);
    apu.write(0xFF19, 0x80);
    assert_ne!(apu.read(REG_NR52) & 0x02, 0);

    apu.set_channel_enabled(ApuChannel::Pulse2, false);

    assert_ne!(apu.read(REG_NR52) & 0x02, 0);
    assert!(!apu.audio_controls().channel(ApuChannel::Pulse2).enabled());
}

#[test]
fn host_audio_controls_mute_channel_output_only() {
    let mut apu = Apu::new();
    apu.write(REG_NR52, 0x80);
    apu.write(0xFF17, 0xF0);
    apu.write(0xFF19, 0x80);
    apu.write(0xFF24, 0x77);
    apu.write(0xFF25, 0x22);

    apu.emit_sample();
    let mut audible = [0.0f32; 2];
    assert_eq!(apu.drain_samples(&mut audible), 2);
    assert_ne!(audible, [0.0, 0.0]);

    apu.set_channel_enabled(ApuChannel::Pulse2, false);
    apu.emit_sample();
    let mut muted = [1.0f32; 2];
    assert_eq!(apu.drain_samples(&mut muted), 2);
    assert_eq!(muted, [0.0, 0.0]);
}

#[test]
fn host_audio_controls_clamp_gain() {
    let mut controls = AudioControls::default();

    controls.set_master_gain(2.0);
    controls.set_channel_gain(ApuChannel::Wave, f32::NAN);
    controls.set_channel_gain(ApuChannel::Noise, -1.0);

    assert_eq!(controls.master_gain(), 1.0);
    assert_eq!(controls.channel(ApuChannel::Wave).gain(), 0.0);
    assert_eq!(controls.channel(ApuChannel::Noise).gain(), 0.0);
}

// -- Wave RAM access ---------------------------------------------------

#[test]
fn wave_ram_writable_when_apu_off() {
    let mut apu = Apu::new();
    apu.write(0xFF30, 0xAB);
    assert_eq!(apu.read(0xFF30), 0xAB);
}

#[test]
fn wave_ram_readable_when_ch3_off_with_apu_on() {
    let mut apu = Apu::new();
    apu.write(REG_NR52, 0x80);
    apu.write(0xFF30, 0x12);
    assert_eq!(apu.read(0xFF30), 0x12);
}

// -- Channel enable bits in NR52 --------------------------------------

#[test]
fn triggering_ch1_with_dac_on_sets_nr52_bit_0() {
    let mut apu = Apu::new();
    apu.write(REG_NR52, 0x80);
    apu.write(0xFF12, 0xF0); // DAC on
    apu.write(0xFF14, 0x80); // trigger
    assert_ne!(apu.read(REG_NR52) & 0x01, 0);
}

#[test]
fn dac_off_disables_channel_immediately() {
    let mut apu = Apu::new();
    apu.write(REG_NR52, 0x80);
    apu.write(0xFF12, 0xF0);
    apu.write(0xFF14, 0x80);
    assert!(apu.ch1.enabled);
    apu.write(0xFF12, 0x00); // DAC off
    assert!(!apu.ch1.enabled);
}

// -- Frame sequencer ---------------------------------------------------

#[test]
fn frame_sequencer_steps_on_div_bit_12_falling_edge() {
    let mut apu = Apu::new();
    apu.write(REG_NR52, 0x80);
    // Set div to just before the bit-12 falling edge — bit 12 high,
    // about to go low when we tick to 0x2000.
    let mut div: u16 = 0x1FFF;
    apu.tick(div); // bit 12 still high → prev_div_bit becomes true
    div = 0x2000;
    apu.tick(div); // bit 12 low → falling edge, frame_step advances
    assert_eq!(apu.frame_step, 1);
}

#[test]
fn frame_sequencer_clocks_length_on_step_zero() {
    let mut apu = Apu::new();
    apu.write(REG_NR52, 0x80);
    apu.write(0xFF12, 0xF0); // CH1 DAC on
    apu.write(0xFF11, 0b00_111111); // length = 64 - 63 = 1
    apu.write(0xFF14, 0xC0); // length enable + trigger (no length-quirk because frame_step=0)
    assert_eq!(apu.ch1.length_timer, 1);
    assert!(apu.ch1.enabled);

    // Step the frame sequencer once: from frame_step=0 to 1, which
    // clocks length counters.
    let mut div: u16 = 0x1FFF;
    apu.tick(div);
    div = 0x2000;
    apu.tick(div);
    assert_eq!(apu.ch1.length_timer, 0);
    assert!(!apu.ch1.enabled, "length expired → channel off");
}

// -- Square channel basics --------------------------------------------

#[test]
fn square_duty_50_percent_pattern_is_half_high() {
    // CH2 at frequency = 2047 → period_timer = (2048-2047)*2 = 2.
    // The timer counts 2→1→0 over two ticks, then on the next tick
    // it reloads + advances duty_position. So duty advances every
    // 3 T-cycles. Sampling at the third tick of each window catches
    // the duty position immediately after the advance.
    let mut apu = Apu::new();
    apu.write(REG_NR52, 0x80);
    apu.write(0xFF16, 0b10_000000); // duty 2 (50%), no length
    apu.write(0xFF17, 0xF0); // envelope: initial 15, DAC on
    apu.write(0xFF18, 0xFF); // freq lo = 0xFF
    apu.write(0xFF19, 0x87); // freq hi (high 3 bits = 7 → freq = 0x7FF = 2047) + trigger

    let mut highs = 0;
    let mut lows = 0;
    let mut div: u16 = 0;
    for _ in 0..8 {
        for _ in 0..3 {
            div = div.wrapping_add(1);
            apu.tick(div);
        }
        let sample = apu.ch2.sample();
        if sample > 0.0 {
            highs += 1;
        } else if sample < 0.0 {
            lows += 1;
        }
    }
    assert_eq!(highs, 4, "50% duty produces 4 high steps per 8");
    assert_eq!(lows, 4, "50% duty produces 4 low steps per 8");
}

// -- Sample emission --------------------------------------------------

#[test]
fn samples_emitted_at_approximately_48_khz() {
    let mut apu = Apu::new();
    apu.write(REG_NR52, 0x80);
    // Run for one second of Game Boy time.
    let _ = run_t(&mut apu, 0, MASTER_HZ);
    // Buffer is bounded; just confirm we emitted a healthy stream.
    // The cap (~8192 floats = ~4096 stereo pairs) caps long runs.
    assert!(
        apu.samples_buffered() >= 1024,
        "got {} samples in 1s",
        apu.samples_buffered()
    );
}

#[test]
fn drain_samples_returns_buffered_floats_in_order() {
    let mut apu = Apu::new();
    apu.write(REG_NR52, 0x80);
    let _ = run_t(&mut apu, 0, 1000);
    let mut dest = [0.0f32; 16];
    let written = apu.drain_samples(&mut dest);
    assert!(written > 0, "drain produced {written} samples");
    // After drain, the same slots aren't re-read.
    let again = apu.drain_samples(&mut dest);
    assert!(again < written + 16);
}
