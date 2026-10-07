//! All channels running: attachment arrival versus the target's byte deadline.
use emu198x_commodore_paula_8364::{AudioField, Paula8364, PaulaAudioDmaState};
use std::collections::BTreeSet;

#[test]
fn running_targets_match_reference_deadlines_and_samples() {
    check(include_str!(
        "../../../test-data/commodore/amiga/paula-audio/active-target-probe/winuae.csv"
    ));
}

fn check(csv: &str) {
    let reference: Vec<Vec<u32>> = csv
        .lines()
        .map(|line| {
            line.split(',')
                .map(|v| v.parse().expect("reference integer"))
                .collect()
        })
        .collect();
    assert_eq!(reference.len(), 88_704);
    assert!(reference.iter().all(|r| r.len() == 17));
    let mut rows = 0;
    let mut cases = BTreeSet::new();
    let mut differences = [0usize; 8];
    let mut controls = [0usize; 8];
    let mut audible = 0;
    while rows < reference.len() {
        let key = reference[rows][..7].to_vec();
        assert!(cases.insert(key.clone()));
        let [source, period, target_period, mode, chain, dma, word] = key[..] else {
            panic!("scenario key")
        };
        assert!(source < 3 && [2, 8, 124].contains(&period));
        assert!((period - 1..=period + 1).contains(&target_period));
        assert!(
            [0, 1, 16, 17].contains(&mode) && chain <= 1 && dma <= 1 && [0, 1, 8].contains(&word)
        );
        let mut p = Paula8364::new();
        for nr in 0..4u8 {
            p.write_audio(
                nr,
                AudioField::Per,
                if u32::from(nr) == source {
                    period
                } else {
                    target_period
                } as u16,
            );
            p.write_audio(nr, AudioField::Vol, 64);
            p.write_audio(nr, AudioField::Len, 64);
        }
        let enabled = if dma != 0 { 0x20f } else { 0 };
        p.sync_audio_dma_control(enabled);
        for nr in 0..4u8 {
            let initial = 0x2030 + u16::from(nr) * 0x111;
            if dma != 0 {
                for val in [0xdead, initial] {
                    let (address, reload) = p.audio_dma_request(nr).expect("startup grant");
                    p.service_audio_dma_word(nr, address, reload, val);
                }
            } else {
                p.write_audio(nr, AudioField::Dat, initial);
            }
        }
        let mut attached = 0;
        for nr in source..3 {
            attached |= mode << nr;
            if chain == 0 {
                break;
            }
        }
        p.write_adkcon(0x8000 | attached as u16);
        for nr in source..3 {
            if dma != 0 {
                let (address, reload) = p.audio_dma_request(nr as u8).expect("holding grant");
                p.service_audio_dma_word(nr as u8, address, reload, word as u16);
            } else {
                p.write_audio(nr as u8, AudioField::Dat, word as u16);
            }
            if chain == 0 {
                break;
            }
        }
        let first_row = rows;
        for time in 0..=4 * period.max(target_period) + 1 {
            let mut pulse = 0;
            if time != 0 {
                p.begin_audio_cck();
                pulse = (p.intreq() >> 7) & 15;
                p.write_intreq(0x780);
                p.finish_audio_cck(enabled, None, |_| panic!("no unscheduled memory grant"));
            }
            if rows == reference.len() || reference[rows][..7] != key || reference[rows][7] != time
            {
                continue;
            }
            let channels = p.audio_diagnostic_snapshot().channels;
            for (nr, ch) in channels.iter().enumerate() {
                assert_eq!(&reference[rows][..7], key);
                assert_eq!(&reference[rows][7..9], &[time, nr as u32]);
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
                    ch.effective_period,
                    ch.period_counter,
                    u32::from(ch.current_word.unwrap_or(0)),
                    ch.output_sample as u8 as u32,
                    u32::from(ch.volume),
                    u32::from(ch.dma_requests_pending != 0),
                    u32::from((pulse >> nr) & 1),
                ];
                let expected = &reference[rows][9..];
                for i in 0..8 {
                    differences[i] += usize::from(actual[i] != expected[i]);
                    if mode == 0 {
                        controls[i] += usize::from(actual[i] != expected[i]);
                    }
                }
                if actual != expected && differences.iter().sum::<usize>() < 30 {
                    eprintln!(
                        "input={:?} actual={actual:?} expected={expected:?}",
                        &reference[rows][..9]
                    );
                }
                if attached & (0x11 << nr) == 0 && actual[4] != expected[4] {
                    audible += 1;
                }
                rows += 1;
            }
        }
        assert!(rows > first_row);
        assert!(
            rows == reference.len() || reference[rows][..7] != key,
            "unconsumed observation"
        );
    }
    assert_eq!(cases.len(), 1296);
    eprintln!(
        "Compared {rows} observations across {} scenarios",
        cases.len()
    );
    eprintln!("state,period,counter,buffer,sample,volume,request,irq: {differences:?}");
    eprintln!("Ordinary controls: {controls:?}; unmuted sample mismatches: {audible}");
    assert_eq!(differences, [0; 8]);
}

#[test]
#[should_panic(expected = "assertion `left == right` failed")]
fn empty_reference_is_rejected() {
    check("");
}

#[test]
#[should_panic(expected = "assertion `left == right` failed")]
fn one_corrupted_deadline_is_rejected() {
    let csv = include_str!(
        "../../../test-data/commodore/amiga/paula-audio/active-target-probe/winuae.csv"
    );
    let first = csv.lines().next().expect("nonempty fixture");
    let mut columns: Vec<String> = first.split(',').map(str::to_owned).collect();
    columns[11] = (columns[11].parse::<u32>().expect("counter") + 1).to_string();
    let corrupted = csv.replacen(first, &columns.join(","), 1);
    check(&corrupted);
}
