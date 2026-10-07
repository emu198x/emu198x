//! Compare target register latches and DMA requests with compiled vAmiga
//! transitions. The driver services actual requests on a one- or five-CCK cadence;
//! this isolates Paula from Agnus slot arbitration and analogue output.

use emu198x_commodore_paula_8364::{AudioField, Paula8364, bits::*};

const WORDS: [u16; 8] = [
    0x0011, 0x0122, 0x0233, 0x0344, 0x0400, 0x0501, 0x067f, 0x0720,
];

#[test]
fn modulation_registers_and_requests_match_reference_transitions() {
    let reference: Vec<Vec<u32>> = include_str!(
        "../../../test-data/commodore/amiga/paula-audio/modulation-probe/reference.csv"
    )
    .lines()
    .map(|line| {
        line.split(',')
            .map(|v| v.parse().expect("reference integer"))
            .collect()
    })
    .collect();
    assert_eq!(reference.len(), 1440, "all reference observations must run");
    let mut observed = Vec::new();
    let mut differences = [0; 4];
    let mut cases = 0;
    for mode in 0u16..4 {
        for period in [1u16, 8, 124] {
            for grant_interval in [1u32, 5] {
                for source in 0usize..4 {
                    cases += 1;
                    let mut p = Paula8364::new();
                    for channel in 0..4 {
                        p.write_audio(channel, AudioField::Per, 777);
                        p.write_audio(channel, AudioField::Vol, 7);
                    }
                    p.write_audio(source as u8, AudioField::Per, period);
                    p.write_audio(source as u8, AudioField::LcLo, 0x1000);
                    p.write_audio(source as u8, AudioField::Len, 512);
                    let attach = if mode & 1 != 0 {
                        ADKCON_USE_PER[source]
                    } else {
                        0
                    } | if mode & 2 != 0 {
                        ADKCON_USE_VOL[source]
                    } else {
                        0
                    };
                    p.write_adkcon(INT_SETCLR | attach);
                    let dma = DMA_MASTER | DMA_AUD[source];
                    p.tick_audio_cck(dma, None, |_| 0);
                    // Dummy fetch then the first real data word. The probe's
                    // origin is the 101 -> 010 transition, not DMA enable.
                    for word in [0, WORDS[0]] {
                        let (address, reload) =
                            p.audio_dma_request(source as u8).expect("startup request");
                        p.service_audio_dma_word(source as u8, address, reload, word);
                    }
                    let mut delivered = 1;
                    for time in 0..=6 * u32::from(period) {
                        if time != 0 {
                            if time % grant_interval == 0
                                && let Some((address, reload)) = p.audio_dma_request(source as u8)
                            {
                                p.service_audio_dma_word(
                                    source as u8,
                                    address,
                                    reload,
                                    WORDS[delivered % WORDS.len()],
                                );
                                delivered += 1;
                            }
                            p.tick_audio_cck(dma, None, |_| 0);
                        }
                        if time == 0
                            || time % u32::from(period) == 0
                            || time % u32::from(period) == 1
                            || time % u32::from(period) == u32::from(period) - 1
                        {
                            let mut row = vec![
                                source as u32,
                                u32::from(mode),
                                u32::from(period),
                                grant_interval,
                                time,
                                delivered as u32,
                                u32::from(
                                    p.audio_diagnostic_snapshot().channels[source]
                                        .dma_requests_pending,
                                ),
                                u32::from(p.audio_dma_request(source as u8).is_some()),
                            ];
                            for channel in 0..4 {
                                row.push(u32::from(p.read_audio(channel, AudioField::Per)));
                                row.push(u32::from(p.read_audio(channel, AudioField::Vol)));
                            }
                            let expected = &reference[observed.len()];
                            if &row != expected {
                                differences[usize::from(mode)] += 1;
                                eprintln!("native={row:?} reference={expected:?}");
                            }
                            observed.push(row);
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 96);
    assert_eq!(observed.len(), reference.len());
    eprintln!("mismatches by mode (ordinary, period, volume, both): {differences:?}");
    assert_eq!(differences, [0; 4]);
}

#[test]
fn delayed_dma_repeats_the_buffer_and_loads_a_late_word_at_high_byte_entry() {
    for arrival in [9, 12, 15] {
        let mut p = Paula8364::new();
        p.write_audio(0, AudioField::LcLo, 0x1000);
        p.write_audio(0, AudioField::Len, 512);
        p.write_audio(0, AudioField::Per, 8);
        let dma = DMA_MASTER | DMA_AUD0;
        p.tick_audio_cck(dma, None, |_| 0);
        for word in [0, 0x1122] {
            let (address, reload) = p.audio_dma_request(0).expect("startup request");
            p.service_audio_dma_word(0, address, reload, word);
        }
        for time in 1..=48 {
            if time == arrival {
                let (address, reload) = p.audio_dma_request(0).expect("retained request");
                p.service_audio_dma_word(0, address, reload, 0x3344);
            }
            p.tick_audio_cck(dma, None, |_| 0);
            let snapshot = p.audio_diagnostic_snapshot().channels[0];
            let expected = match time {
                1..=7 => 0x11,
                8..=15 => 0x22,
                _ if (time / 8) % 2 == 0 => 0x33,
                _ => 0x44,
            };
            assert_eq!(
                snapshot.output_sample, expected,
                "arrival={arrival} time={time}"
            );
            assert!(
                snapshot.dma_requests_pending <= 1,
                "AUDxDR is a retained line"
            );
        }
    }
}

#[test]
fn combined_tick_and_retained_dma_service_agree_in_every_attach_mode() {
    for source in 0usize..4 {
        for mode in 0u16..4 {
            for period in [1, 8, 124] {
                let mut retained = Paula8364::new();
                retained.write_audio(source as u8, AudioField::LcLo, 0x1000);
                retained.write_audio(source as u8, AudioField::Len, 512);
                retained.write_audio(source as u8, AudioField::Per, period);
                let attach = if mode & 1 != 0 {
                    ADKCON_USE_PER[source]
                } else {
                    0
                } | if mode & 2 != 0 {
                    ADKCON_USE_VOL[source]
                } else {
                    0
                };
                retained.write_adkcon(INT_SETCLR | attach);
                let dma = DMA_MASTER | DMA_AUD[source];
                retained.tick_audio_cck(dma, None, |_| 0);
                let mut combined = retained.clone();
                let word_at = |address: u32| WORDS[((address - 0x1000) / 2) as usize % WORDS.len()];
                for time in 0..1500 {
                    let slot = (time % 5 == 0).then_some(source as u8);
                    retained.begin_audio_cck();
                    if slot.is_some()
                        && let Some((address, reload)) = retained.audio_dma_request(source as u8)
                    {
                        retained.service_audio_dma_word(
                            source as u8,
                            address,
                            reload,
                            word_at(address),
                        );
                    }
                    retained.finish_audio_cck(dma, None, |_| 0);
                    combined.tick_audio_cck(dma, slot, |address| {
                        let word = word_at(address & !1);
                        if address & 1 == 0 {
                            (word >> 8) as u8
                        } else {
                            word as u8
                        }
                    });
                    assert_eq!(
                        retained.audio_diagnostic_snapshot(),
                        combined.audio_diagnostic_snapshot(),
                        "source={source} mode={mode} period={period} time={time}"
                    );
                    assert_eq!(retained.intreq(), combined.intreq());
                    assert_eq!(retained.mix_audio_stereo(), combined.mix_audio_stereo());
                }
            }
        }
    }
}
