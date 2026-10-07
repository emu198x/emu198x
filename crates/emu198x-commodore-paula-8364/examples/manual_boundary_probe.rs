//! Research reproduction: CPU-fed playback against pinned WinUAE transitions.
//! Exits nonzero while any observed output, IRQ or playback state disagrees.

use std::collections::BTreeSet;

use emu198x_commodore_paula_8364::{AudioField, Paula8364, PaulaAudioDmaState, bits::*};

fn apply(p: &mut Paula8364, source: u8, scenario: u32, time: u32, period: u32) {
    let occurs = match scenario {
        0 | 10 => false,
        1 => time == 1,
        2 => time == 2 * period - 2,
        3 => time == 2 * period - 1,
        4 => time == 2 * period,
        5 => time == 2 * period + 1,
        6 => time == 1 || time == 2 * period,
        7 => time == 1 || time == 2 * period - 1,
        8 => time == 1 || time == period / 2,
        9 => time == 1 || time == period + period / 2,
        _ => panic!("unregistered scenario"),
    };
    if !occurs {
        return;
    }
    let irq = INT_AUD0 << source;
    if (scenario == 6 && time == 2 * period) || (scenario == 7 && time == 2 * period - 1) {
        p.write_intreq(INT_SETCLR | irq);
    } else if (scenario == 8 && time == period / 2)
        || (scenario == 9 && time == period + period / 2)
    {
        p.write_audio(source, AudioField::Dat, 0x3344);
    } else {
        p.write_intreq(irq);
    }
}

fn main() {
    let reference: Vec<Vec<i32>> =
        include_str!("../../../test-data/commodore/amiga/paula-audio/manual-probe/winuae.csv")
            .lines()
            .map(|line| {
                line.split(',')
                    .map(|s| s.parse().expect("reference integer"))
                    .collect()
            })
            .collect();
    assert_eq!(reference.len(), 4312);
    let mut rows = 0;
    let mut functional_differences = [0; 11];
    let mut state_differences = 0;
    for period in [1, 2, 8, 124, 65536u32] {
        let points: BTreeSet<_> = [
            0,
            1,
            period - 1,
            period,
            period + 1,
            2 * period - 2,
            2 * period - 1,
            2 * period,
            2 * period + 1,
            2 * period + 2,
            3 * period,
            3 * period + 1,
        ]
        .into_iter()
        .collect();
        for (scenario, differences) in functional_differences.iter_mut().enumerate() {
            for after in [false, true] {
                for source in 0..4u8 {
                    let mut p = Paula8364::new();
                    p.write_audio(source, AudioField::Per, period as u16);
                    p.write_audio(source, AudioField::Vol, 64);
                    if scenario == 10 {
                        p.write_intreq(INT_SETCLR | (INT_AUD0 << source));
                    }
                    p.write_audio(source, AudioField::Dat, 0x1122);
                    for time in 0..=3 * period + 1 {
                        if time != 0 {
                            p.begin_audio_cck();
                            if !after {
                                apply(&mut p, source, scenario as u32, time, period);
                            }
                            p.finish_audio_cck(0, None, |_| {
                                panic!("manual playback must not read DMA")
                            });
                            if after {
                                apply(&mut p, source, scenario as u32, time, period);
                            }
                        }
                        if !points.contains(&time) {
                            continue;
                        }
                        let ch = p.audio_diagnostic_snapshot().channels[usize::from(source)];
                        let state = match ch.state {
                            PaulaAudioDmaState::Idle => 0,
                            PaulaAudioDmaState::Playing => {
                                if ch.next_byte_is_high {
                                    3
                                } else {
                                    2
                                }
                            }
                            _ => panic!("manual playback entered a DMA wait"),
                        };
                        let row = vec![
                            i32::from(source),
                            period as i32,
                            scenario as i32,
                            i32::from(after),
                            time as i32,
                            state,
                            i32::from(p.intreq() & (INT_AUD0 << source) != 0),
                            i32::from(ch.output_sample),
                        ];
                        assert_eq!(row[..5], reference[rows][..5], "case inventory and clock");
                        if row[6..] != reference[rows][6..] {
                            *differences += 1;
                            if *differences <= 2 {
                                eprintln!("native={row:?} reference={:?}", reference[rows]);
                            }
                        }
                        state_differences += usize::from(row[5] != reference[rows][5]);
                        rows += 1;
                    }
                }
            }
        }
    }
    assert_eq!(rows, reference.len());
    eprintln!("Functional mismatches by scenario: {functional_differences:?}");
    eprintln!("Playback state mismatches: {state_differences}");
    assert_eq!(functional_differences, [0; 11]);
    assert_eq!(state_differences, 0);
}
