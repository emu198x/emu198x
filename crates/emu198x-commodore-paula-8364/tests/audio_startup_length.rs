//! Independent startup-length observations; grants are explicit, not full-board bus timing.
use emu198x_commodore_paula_8364::{AudioField, Paula8364, bits::*};

fn requests(source: u8, length: u16, period: u16, admitted: bool) -> Vec<u32> {
    let mut p = Paula8364::new();
    p.write_audio(source, AudioField::Len, length);
    p.write_audio(source, AudioField::Per, period);
    let dma = DMA_MASTER | DMA_AUD[usize::from(source)];
    let irq = INT_AUD0 << source;
    let mut times = Vec::new();
    for time in 0..250_000 {
        p.begin_audio_cck();
        let slot = (time % 227 == 14).then_some(source);
        if admitted
            && slot.is_some()
            && let Some((address, reload)) = p.audio_dma_request(source)
        {
            p.service_audio_dma_word(source, address, reload, 0x1122);
        }
        p.finish_audio_cck(dma, if admitted { None } else { slot }, |address| {
            if address & 1 == 0 { 0x11 } else { 0x22 }
        });
        if p.intreq() & irq != 0 {
            times.push(time);
            p.write_intreq(irq);
            if times.len() == 4 {
                break;
            }
        }
    }
    assert_eq!(times.len(), 4, "bounded probe must observe four requests");
    times
}

#[test]
fn first_loop_and_later_cadence_match_both_references_on_every_channel() {
    let rows: Vec<Vec<u32>> = include_str!(
        "../../../test-data/commodore/amiga/paula-audio/startup-length-probe/reference.csv"
    )
    .lines()
    .map(|line| {
        line.split(',')
            .map(|n| n.parse().expect("reference integer"))
            .collect()
    })
    .collect();
    assert_eq!(rows.len(), 32);
    let (cases, remainder) = rows.as_chunks::<4>();
    assert!(remainder.is_empty());
    for (case, expected) in cases.iter().enumerate() {
        let length = [1, 2, 8, 64][case / 2];
        let period = [124, 424][case % 2];
        for (index, row) in expected.iter().enumerate() {
            assert_eq!(
                &row[..3],
                &[u32::from(length), u32::from(period), index as u32]
            );
        }
        for source in 0..4 {
            let direct = requests(source, length, period, false);
            let admitted = requests(source, length, period, true);
            assert_eq!(direct, admitted, "direct/admitted path parity");
            assert_eq!(direct[0], expected[0][3]);
            for index in 1..4 {
                // The native output phase clocks the newly loaded period on
                // the DAT-arrival CCK; reference event schedulers start its
                // countdown on the next CCK. Preserve that existing phase.
                assert_eq!(
                    direct[index] + 1,
                    expected[index][3],
                    "channel {source}, LEN {length}, PER {period}, IRQ {index}"
                );
            }
            for index in 2..4 {
                assert_eq!(
                    direct[index] - direct[index - 1],
                    u32::from(length) * 2 * u32::from(period)
                );
            }
        }
    }
}

#[test]
fn first_and_later_buffers_fetch_every_word_including_zero_and_one_length() {
    for source in 0..4 {
        for length in [0u16, 1, 2, 8] {
            let mut p = Paula8364::new();
            p.write_audio(source, AudioField::Len, length);
            p.write_audio(source, AudioField::Per, 1);
            let dma = DMA_MASTER | DMA_AUD[usize::from(source)];
            let count = if length == 0 {
                65_536
            } else {
                u32::from(length)
            };
            let mut fetches = 0;
            for _ in 0..(count * 4 + 20) {
                p.begin_audio_cck();
                if let Some((address, reload)) = p.audio_dma_request(source) {
                    let expected = if fetches == 0 {
                        0
                    } else {
                        ((fetches - 1) % count) * 2
                    };
                    assert_eq!(
                        address, expected,
                        "channel {source}, LEN {length}, fetch {fetches}"
                    );
                    p.service_audio_dma_word(source, address, reload, 0x1122);
                    fetches += 1;
                }
                p.finish_audio_cck(dma, None, |_| panic!("unexpected direct fetch"));
                if fetches == 2 * count + 1 {
                    break;
                }
            }
            assert_eq!(fetches, 2 * count + 1, "dummy plus two complete buffers");
        }
    }
}
