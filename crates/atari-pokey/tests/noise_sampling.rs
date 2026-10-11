use atari_pokey::Pokey;

fn run(chip: &mut Pokey, ticks: u32) {
    for _ in 0..ticks {
        chip.tick();
    }
}

#[test]
fn noise_holds_between_timer_events() {
    let mut cases = 0;
    for clock in [1_789_772, 1_773_447] {
        for (audctl, divider) in [(0, 28), (1, 114)] {
            for channel in 0..4 {
                for distortion in 0..8 {
                    let mut chip = Pokey::new(clock);
                    chip.write(0x0f, 3);
                    chip.write(0x08, audctl);
                    chip.write(channel * 2, 255);
                    chip.write(channel * 2 + 1, distortion << 5 | 15);
                    chip.write(0x09, 0);
                    // Observe only the interval between the first and second
                    // timer events, after the first host sample has settled.
                    run(&mut chip, 256 * divider + 128);
                    chip.take_buffer();
                    chip.take_channel_buffers();
                    run(&mut chip, 6000);
                    let taps = chip.take_channel_buffers();
                    let output = &taps[usize::from(channel)];
                    assert!(output.len() > 150);
                    assert!(
                        output.iter().all(|&s| s == output[0]),
                        "noise changed without a timer event: clock={clock} AUDCTL={audctl} channel={channel} distortion={distortion}"
                    );
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 128);
}

#[test]
fn noise_period_aliases_hold_a_constant_output() {
    let mut cases = 0;
    for clock in [1_789_772, 1_773_447] {
        for (audctl, distortion, period) in [
            (0, 0xcf, 14),
            (0, 0xcf, 29),
            (0, 0xcf, 44),
            (0x80, 0x8f, 72),
        ] {
            for channel in 0..4 {
                let mut chip = Pokey::new(clock);
                chip.write(0x0f, 3);
                chip.write(0x08, audctl);
                chip.write(channel * 2, period);
                chip.write(channel * 2 + 1, distortion);
                chip.write(0x09, 0);
                run(&mut chip, 8192);
                chip.take_buffer();
                chip.take_channel_buffers();
                run(&mut chip, 32_768);
                let taps = chip.take_channel_buffers();
                let output = &taps[usize::from(channel)];
                assert!(output.len() > 800);
                assert!(
                    output.iter().all(|&s| s == output[0]),
                    "sampling a repeated noise bit must stay constant: clock={clock} channel={channel} AUDC={distortion:02x} AUDF={period}"
                );
                // Detuning by one divider tick must restore changing noise.
                // This prevents an always-silent implementation passing.
                chip.write(channel * 2, period + 1);
                chip.write(0x09, 0);
                run(&mut chip, 8192);
                chip.take_buffer();
                chip.take_channel_buffers();
                run(&mut chip, 32_768);
                let taps = chip.take_channel_buffers();
                let output = &taps[usize::from(channel)];
                assert!(output.len() > 800);
                let low = output.iter().copied().fold(f32::INFINITY, f32::min);
                let high = output.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                assert!(high - low > 0.25, "detuned noise must vary");
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 32);
}
