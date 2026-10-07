//! Pinned WinUAE/vAmiga DAT delivery after cancelled DMA startup.
use emu198x_commodore_paula_8364::{AudioField, Paula8364, PaulaAudioDmaState, bits::*};

#[test]
fn retained_word_after_cancelled_startup_matches_reference_and_cpu_dat() {
    let reference: Vec<Vec<i32>> = include_str!(
        "../../../test-data/commodore/amiga/paula-audio/cancelled-startup-probe/winuae.csv"
    )
    .lines()
    .map(|line| {
        line.split(',')
            .map(|v| v.parse().expect("reference integer"))
            .collect()
    })
    .collect();
    assert_eq!(reference.len(), 704);
    let mut mismatches = [0; 2];
    for (path, failures) in mismatches.iter_mut().enumerate() {
        let mut rows = 0;
        for period in [1u32, 2, 8, 124, 65_536] {
            for second in [false, true] {
                for pending in [false, true] {
                    for source in 0..4u8 {
                        let mut p = Paula8364::new();
                        let irq = INT_AUD0 << source;
                        let enabled = DMA_MASTER | DMA_AUD[usize::from(source)];
                        p.write_audio(source, AudioField::Per, period as u16);
                        p.write_audio(source, AudioField::Len, 64);
                        p.write_audio(source, AudioField::LcLo, 0x1000);
                        p.tick_audio_cck(enabled, None, |_| panic!("no autonomous grant"));
                        if second {
                            let (address, reload) =
                                p.audio_dma_request(source).expect("dummy request");
                            p.service_audio_dma_word(source, address, reload, 0xdead);
                        }
                        let (address, reload) =
                            p.audio_dma_request(source).expect("retained request");
                        let initial_length = p.audio_diagnostic_snapshot().channels
                            [usize::from(source)]
                        .words_remaining;
                        let initial_ref_length = reference[rows][9];
                        for time in 0..=2 * period + 3 {
                            if time != 0 {
                                p.tick_audio_cck(0, None, |_| panic!("no new grants"));
                                if time == 1 {
                                    p.write_intreq(irq | if pending { 0x8000 } else { 0 });
                                }
                                if time == 2 {
                                    if path == 0 {
                                        p.write_audio(source, AudioField::Dat, 0x1122);
                                    } else {
                                        p.service_audio_dma_word(source, address, reload, 0x1122);
                                    }
                                }
                            }
                            if rows == reference.len()
                                || reference[rows][..4]
                                    != [
                                        i32::from(source),
                                        period as i32,
                                        i32::from(second),
                                        i32::from(pending),
                                    ]
                                || reference[rows][4] != time as i32
                            {
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
                            let actual = [
                                state,
                                i32::from(p.intreq() & irq != 0),
                                i32::from(ch.output_sample),
                                i32::from(p.read_audio(source, AudioField::Dat)),
                                ch.words_remaining as i32 - initial_length as i32,
                            ];
                            let expected = [
                                reference[rows][5],
                                reference[rows][6],
                                reference[rows][7],
                                reference[rows][8],
                                reference[rows][9] - initial_ref_length,
                            ];
                            if actual != expected {
                                *failures += 1;
                                if *failures <= 4 {
                                    eprintln!(
                                        "path={path} input={:?} actual={actual:?} expected={expected:?}",
                                        &reference[rows][..5]
                                    );
                                }
                            }
                            rows += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(
            rows,
            reference.len(),
            "all reference observations must execute"
        );
    }
    eprintln!("CPU DAT / retained DMA mismatches: {mismatches:?}");
    assert_eq!(mismatches, [0, 0]);
}
