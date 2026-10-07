//! Explicit live-attachment differential diagnostic. Supply the generated WinUAE CSV.
use emu198x_commodore_paula_8364::{AudioField, Paula8364, PaulaAudioDmaState};
use std::{collections::BTreeSet, error::Error};

pub fn check(reference: &str) -> Result<(), Box<dyn Error>> {
    let reference: Vec<Vec<i32>> = reference
        .lines()
        .map(|line| {
            line.split(',')
                .map(str::parse)
                .collect::<Result<Vec<i32>, _>>()
        })
        .collect::<Result<_, _>>()?;
    assert_eq!(reference.len(), 60_928);
    assert!(reference.iter().all(|row| row.len() == 17));
    let mut rows = 0;
    let mut cases = BTreeSet::new();
    let mut differences = [0; 9];
    let mut audible = 0;
    let mut controls = [0; 9];
    while rows < reference.len() {
        let start_row = rows;
        let key = reference[rows][..7].to_vec();
        assert!(cases.insert(key.clone()), "duplicate scenario");
        let [source, period, dma, from, to, edge, first] = key[..] else {
            panic!("scenario key")
        };
        assert!((0..4).contains(&source));
        assert!([1, 2, 8, 124].contains(&period));
        assert!([0, 1].contains(&dma) && [0, 1].contains(&first));
        assert!([0, 1, 16, 17].contains(&from) && [0, 1, 16, 17].contains(&to));
        let mut p = Paula8364::new();
        let channel = source as u8;
        let irq = 0x80 << channel;
        for ch in 0..4 {
            p.write_audio(ch, AudioField::Per, 31);
            p.write_audio(ch, AudioField::Vol, 64);
        }
        p.write_audio(channel, AudioField::Per, period as u16);
        p.write_audio(channel, AudioField::Len, 1);
        p.write_adkcon(0x8000 | ((from as u16) << channel));
        let enabled = if dma != 0 { 0x0200 | (1 << channel) } else { 0 };
        if dma != 0 {
            p.sync_audio_dma_control(enabled);
            for word in [0xdead, 0x1122] {
                let (address, reload) = p.audio_dma_request(channel).expect("startup request");
                p.service_audio_dma_word(channel, address, reload, word);
            }
        } else {
            p.write_audio(channel, AudioField::Dat, 0x1122);
        }
        let switch = |p: &mut Paula8364| {
            p.write_adkcon(0x11 << channel);
            p.write_adkcon(0x8000 | ((to as u16) << channel));
        };
        let mut delivered = false;
        let mut pulse = false;
        for time in 0..=4 * period + 1 {
            if time != 0 {
                p.begin_audio_cck();
                pulse = p.intreq() & irq != 0;
                p.write_intreq(irq);
                if time == edge && first != 0 {
                    switch(&mut p);
                }
                if !delivered && time >= edge {
                    if dma == 0 {
                        p.write_audio(channel, AudioField::Dat, 0x3344);
                        delivered = true;
                    } else if let Some((address, reload)) = p.audio_dma_request(channel) {
                        p.service_audio_dma_word(channel, address, reload, 0x3344);
                        delivered = true;
                    }
                }
                if time == edge && first == 0 {
                    switch(&mut p);
                }
                p.finish_audio_cck(enabled, None, |_| panic!("no autonomous memory grants"));
            }
            if rows == reference.len() || reference[rows][..7] != key || reference[rows][7] != time
            {
                continue;
            }
            let channels = p.audio_diagnostic_snapshot().channels;
            let ch = channels[usize::from(channel)];
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
            let (target_period, target_volume) = if channel < 3 {
                let target = channels[usize::from(channel + 1)];
                (target.effective_period as i32, i32::from(target.volume))
            } else {
                (0, 0)
            };
            let actual = [
                state,
                i32::from(pulse),
                i32::from(ch.current_word.unwrap_or(0)),
                i32::from(ch.output_sample),
                target_period,
                target_volume,
                i32::from(ch.dma_requests_pending != 0),
                i32::from(ch.loop_interrupt_pending),
                i32::from(delivered),
            ];
            let expected = &reference[rows][8..];
            for i in 0..9 {
                differences[i] += usize::from(actual[i] != expected[i]);
                if from == to {
                    controls[i] += usize::from(actual[i] != expected[i]);
                }
            }
            let mode = if time < edge { from } else { to };
            if mode == 0 && actual[3] != expected[3] {
                audible += 1;
                if audible <= 12 {
                    eprintln!(
                        "audible input={:?} actual={actual:?} reference={expected:?}",
                        reference[rows][..8].to_vec()
                    );
                }
            }
            if mode != 0 {
                assert_eq!(
                    p.mix_audio_stereo(),
                    (0.0, 0.0),
                    "attached source must be muted"
                );
            }
            rows += 1;
        }
        assert!(rows > start_row, "scenario must consume observations");
        assert!(
            rows == reference.len() || reference[rows][..7] != key,
            "unconsumed scenario observation"
        );
    }
    assert_eq!(cases.len(), 5632);
    eprintln!("Compared {rows} observations in {} scenarios", cases.len());
    eprintln!(
        "state,irq,buffer,sample,target_period,target_volume,request,loop,delivered: {differences:?}"
    );
    eprintln!("Unchanged-mode controls: {controls:?}");
    eprintln!("Audible sample mismatches: {audible}");
    assert_eq!(differences, [0; 9]);
    Ok(())
}
