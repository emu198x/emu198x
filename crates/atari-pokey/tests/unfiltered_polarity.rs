//! Altirra Hardware Reference Manual p108: disabled HPF still inverts ch1/2.
//! Equal pure tones on ch1/2 add; ch1/3 cancel. Volume-only bypasses inversion.
//! Check those relationships at the public audio output, independent of the
//! counters' starting phase. This does not qualify active-filter pulse timing.

use atari_pokey::Pokey;

const CLOCKS: [u32; 2] = [1_789_772, 1_773_447];
const TICKS: u32 = 16_384;
const PAIRS: [(usize, usize); 6] = [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)];

fn capture(clock: u32, audctl: u8, period: u8, controls: [u8; 4]) -> (Vec<f32>, [Vec<f32>; 4]) {
    let mut chip = Pokey::new(clock);
    chip.write(0x0f, 3);
    chip.write(0x08, audctl);
    for (channel, control) in controls.into_iter().enumerate() {
        chip.write(channel as u8 * 2, period);
        chip.write(channel as u8 * 2 + 1, control);
    }
    chip.write(0x09, 0);
    for _ in 0..TICKS {
        chip.tick();
    }
    let mono = chip.take_buffer();
    let taps = chip.take_channel_buffers();
    assert_eq!(
        mono.len(),
        (u64::from(TICKS) * 48_000 / u64::from(clock)) as usize
    );
    assert!(
        mono.len() > 400,
        "a silent or empty capture is not evidence"
    );
    assert!(taps.iter().all(|tap| tap.len() == mono.len()));
    (mono, taps)
}

#[test]
fn synchronized_tones_add_or_cancel_according_to_channel_polarity() {
    let mut cases = 0;
    for clock in CLOCKS {
        for audctl in [0, 1] {
            let (constant_mix, _) = capture(clock, audctl, 0, [0x1f, 0, 0, 0]);
            for period in [0, 3, 15] {
                for tone in [0xaf, 0xef] {
                    for (a, b) in PAIRS {
                        let mut controls = [0; 4];
                        controls[a] = tone;
                        controls[b] = tone;
                        let (mono, taps) = capture(clock, audctl, period, controls);
                        let opposite = (a < 2) != (b < 2);
                        let context = format!(
                            "clock={clock} AUDCTL={audctl} AUDF={period} AUDC={tone:02x} pair={a},{b}"
                        );
                        for channel in [a, b] {
                            let low = taps[channel].iter().copied().fold(f32::INFINITY, f32::min);
                            let high = taps[channel]
                                .iter()
                                .copied()
                                .fold(f32::NEG_INFINITY, f32::max);
                            assert!(high - low > 0.25, "{context}: tone must vary");
                        }
                        for (sample, (&left, &right)) in taps[a].iter().zip(&taps[b]).enumerate() {
                            if opposite {
                                assert!(
                                    (left + right - 1.0).abs() < 1e-6,
                                    "{context} sample={sample}: opposite outputs must cancel, got {left}+{right}"
                                );
                            } else {
                                assert_eq!(
                                    left, right,
                                    "{context} sample={sample}: same-polarity outputs must agree"
                                );
                            }
                        }
                        if opposite {
                            assert_eq!(
                                mono, constant_mix,
                                "{context}: mixed output must equal the constant-level control"
                            );
                        } else {
                            assert!(
                                mono.iter()
                                    .zip(&constant_mix)
                                    .any(|(x, y)| (x - y).abs() > 0.05),
                                "{context}: adding tones must remain audible"
                            );
                        }
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 144);
}

#[test]
fn volume_only_bypasses_polarity_for_every_channel_pair() {
    let mut cases = 0;
    for clock in CLOCKS {
        for audctl in [0, 1] {
            let (constant_mix, _) = capture(clock, audctl, 0, [0x1f, 0x1f, 0, 0]);
            for volume_only in [0x1f, 0xbf] {
                for (a, b) in PAIRS {
                    let mut controls = [0; 4];
                    controls[a] = volume_only;
                    controls[b] = volume_only;
                    let (mono, taps) = capture(clock, audctl, 3, controls);
                    assert_eq!(mono, constant_mix);
                    for (channel, tap) in taps.iter().enumerate() {
                        let expected = if channel == a || channel == b {
                            1.0
                        } else {
                            0.0
                        };
                        assert!(tap.iter().all(|&sample| sample == expected));
                    }
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 48);
}
