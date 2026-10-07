//! Research reproduction of DMA/manual handover; fails on observed disagreements.
use emu198x_commodore_paula_8364::{AudioField, Paula8364, PaulaAudioDmaState, bits::*};
use std::collections::BTreeSet;

fn main() {
    let reference: Vec<Vec<i32>> =
        include_str!("../../../test-data/commodore/amiga/paula-audio/handover-probe/winuae.csv")
            .lines()
            .map(|line| {
                line.split(',')
                    .map(|v| v.parse().expect("reference integer"))
                    .collect::<Vec<i32>>()
            })
            .filter(|row| row[4] == 0)
            .collect();
    assert_eq!(reference.len(), 6304);
    let mut rows = 0;
    let mut functional = [0; 8];
    let mut states = [0; 8];
    let mut loops = [0; 8];
    for period in [1u32, 2, 8, 124] {
        for scenario in 0..8 {
            let edges: BTreeSet<_> = [
                1,
                period.saturating_sub(1).max(1),
                period,
                period + 1,
                2 * period - 1,
                2 * period,
                2 * period + 1,
            ]
            .into_iter()
            .collect();
            for edge in edges {
                for source in 0..4u8 {
                    let mut p = Paula8364::new();
                    p.write_audio(source, AudioField::Per, period as u16);
                    p.write_audio(source, AudioField::Len, if scenario == 7 { 1 } else { 64 });
                    p.write_audio(source, AudioField::Vol, 64);
                    let irq = INT_AUD0 << source;
                    let enabled = DMA_MASTER | DMA_AUD[usize::from(source)];
                    let mut dma = 0;
                    if scenario == 0 || scenario == 6 {
                        p.write_audio(source, AudioField::Dat, 0x1122);
                    } else {
                        dma = enabled;
                        p.tick_audio_cck(dma, None, |_| panic!("unexpected DMA fetch"));
                        for word in [0xdead, 0x1122] {
                            let (address, reload) =
                                p.audio_dma_request(source).expect("startup request");
                            p.service_audio_dma_word(source, address, reload, word);
                        }
                        if scenario == 4 || scenario == 7 {
                            let (address, reload) =
                                p.audio_dma_request(source).expect("next word request");
                            p.service_audio_dma_word(source, address, reload, 0x3344);
                        }
                    }
                    let points: BTreeSet<_> = [
                        0,
                        1,
                        edge - 1,
                        edge,
                        edge + 1,
                        edge + 2,
                        period,
                        2 * period - 1,
                        2 * period,
                        2 * period + 1,
                        3 * period,
                        3 * period + 1,
                    ]
                    .into_iter()
                    .collect();
                    for time in 0..=3 * period + 1 {
                        if time != 0 {
                            p.begin_audio_cck();
                            if (scenario == 7 && time == 1)
                                || scenario == 2
                                || scenario == 4
                                || ((scenario == 5 || scenario == 6) && time == 2 * period)
                            {
                                p.write_intreq(irq);
                            }
                            if time == edge {
                                dma = if scenario == 0 || scenario == 6 {
                                    enabled
                                } else {
                                    0
                                };
                            }
                            if (scenario == 3 || scenario == 7) && time == edge + 1 {
                                dma = enabled;
                            }
                            if scenario == 6 && time == edge + 1 {
                                dma = 0;
                            }
                            p.finish_audio_cck(dma, None, |_| panic!("probe did not grant DMA"));
                        }
                        if !points.contains(&time) {
                            continue;
                        }
                        let ch = p.audio_diagnostic_snapshot().channels[usize::from(source)];
                        let state = match ch.state {
                            PaulaAudioDmaState::Idle => 0,
                            PaulaAudioDmaState::WaitWord1 => 1,
                            PaulaAudioDmaState::WaitWord2 => 5,
                            PaulaAudioDmaState::Playing => {
                                if ch.next_byte_is_high {
                                    3
                                } else {
                                    2
                                }
                            }
                        };
                        let row = vec![
                            i32::from(source),
                            period as i32,
                            scenario as i32,
                            edge as i32,
                            0,
                            time as i32,
                            state,
                            i32::from(p.intreq() & irq != 0),
                            i32::from(ch.output_sample),
                            i32::from(ch.loop_interrupt_pending),
                        ];
                        assert_eq!(row[..6], reference[rows][..6], "inventory/clock");
                        if row[7..9] != reference[rows][7..9] {
                            functional[scenario] += 1;
                            if functional[scenario] <= 3 {
                                eprintln!("native={row:?} reference={:?}", reference[rows]);
                            }
                        }
                        loops[scenario] += usize::from(row[9] != reference[rows][9]);
                        states[scenario] += usize::from(row[6] != reference[rows][6]);
                        rows += 1;
                    }
                }
            }
        }
    }
    assert_eq!(rows, reference.len());
    eprintln!("Functional mismatches by scenario: {functional:?}");
    eprintln!("Held loop mismatches: {loops:?}");
    eprintln!("State mismatches by scenario: {states:?}");
    assert_eq!(functional, [0; 8]);
    assert_eq!(states, [0; 8]);
    assert_eq!(loops, [0; 8]);
}
