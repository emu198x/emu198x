use atari_pokey::Pokey;

fn run(chip: &mut Pokey, ticks: u32) {
    for _ in 0..ticks {
        chip.tick();
    }
}

/// All tested timers are more than 384 clocks from their next event.
/// Discard the partially accumulated host sample before reading a held level.
fn held_level(chip: &mut Pokey, channel: u8) -> f32 {
    run(chip, 128);
    chip.take_buffer();
    chip.take_channel_buffers();
    run(chip, 256);
    let taps = chip.take_channel_buffers();
    let output = &taps[usize::from(channel)];
    assert!(output.len() >= 6);
    assert!(output[0] == 0.0 || output[0] == 1.0);
    assert!(output.iter().all(|&sample| sample == output[0]));
    output[0]
}

fn configured(clock: u32, base: u8, channel: u8, other_filter: u8) -> Pokey {
    let mut chip = Pokey::new(clock);
    chip.write(0x0f, 3);
    chip.write(0x08, base | other_filter);
    for ch in 0..4 {
        chip.write(ch * 2, 255);
    }
    chip.write(channel * 2 + 1, 0xaf);
    chip.write(0x09, 0);
    chip
}

#[test]
fn first_filter_enable_preserves_the_disabled_output() {
    let mut cases = 0;
    for clock in [1_789_772, 1_773_447] {
        for base in [0, 1] {
            for (channel, mask, other_mask) in [(0, 4, 2), (1, 2, 4)] {
                for other in [0, other_mask] {
                    let mut chip = configured(clock, base, channel, other);
                    let before = held_level(&mut chip, channel);
                    chip.write(0x08, base | other | mask);
                    assert_eq!(
                        held_level(&mut chip, channel),
                        before,
                        "enabling cannot resurrect a low disabled latch: clock={clock} base={base} channel={channel} other={other}"
                    );
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 16);
}

#[test]
fn old_saves_with_a_low_disabled_latch_re_enable_without_a_jump() {
    let mut cases = 0;
    for clock in [1_789_772, 1_773_447] {
        for base in [0, 1] {
            for (channel, mask, other_mask) in [(0, 4, 2), (1, 2, 4)] {
                for other in [0, other_mask] {
                    let chip = configured(clock, base, channel, other);
                    let mut old_state = chip.save_state();
                    // The existing compact format stores eight bytes per
                    // channel; the last byte is the filter latch. Earlier
                    // versions could save zero while the filter was disabled.
                    old_state[usize::from(channel) * 8 + 7] = 0;
                    let mut resumed = Pokey::new(clock);
                    assert_eq!(
                        resumed.load_state(&old_state).expect("old state"),
                        old_state.len()
                    );
                    let disabled = held_level(&mut resumed, channel);
                    resumed.write(0x08, base | other | mask);
                    assert_eq!(
                        held_level(&mut resumed, channel),
                        disabled,
                        "a disabled latch from an older save must be high before enable"
                    );
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 16);
}

#[test]
fn disabled_filter_forgets_the_captured_bit_before_re_enable() {
    let mut cases = 0;
    for clock in [1_789_772, 1_773_447] {
        for (base, divider) in [(0, 28), (1, 114)] {
            for (channel, mask, other_mask) in [(0, 4, 2), (1, 2, 4)] {
                for other in [0, other_mask] {
                    for volume_only in [false, true] {
                        for restore in [false, true] {
                            let mut chip = configured(clock, base, channel, other);
                            chip.write(0x08, base | other | mask);
                            // The paired high timer captures the still-low
                            // waveform, then STIMER parks both timers.
                            chip.write((channel + 2) * 2, 0);
                            chip.write(0x09, 0);
                            run(&mut chip, 3 * divider);
                            chip.write((channel + 2) * 2, 255);
                            chip.write(0x09, 0);
                            assert_eq!(held_level(&mut chip, channel), 0.0);

                            let other = other ^ other_mask;
                            chip.write(0x08, base | other | mask);
                            assert_eq!(
                                held_level(&mut chip, channel),
                                0.0,
                                "changing the other filter must retain this captured bit"
                            );

                            if volume_only {
                                chip.write(channel * 2 + 1, 0xbf);
                            }
                            chip.write(0x08, base | other);
                            let disabled = held_level(&mut chip, channel);
                            assert_eq!(disabled, 1.0, "disable must clear the captured-low result");
                            if restore {
                                let saved = chip.save_state();
                                let mut resumed = Pokey::new(clock);
                                assert_eq!(
                                    resumed.load_state(&saved).expect("direct state"),
                                    saved.len()
                                );
                                chip = resumed;
                            }
                            chip.write(0x08, base | other | mask);
                            chip.write(channel * 2 + 1, 0xaf);
                            assert_eq!(
                                held_level(&mut chip, channel),
                                disabled,
                                "re-enable must retain the disabled level: clock={clock} base={base} channel={channel} volume_only={volume_only} restore={restore}"
                            );
                            cases += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 64);
}
