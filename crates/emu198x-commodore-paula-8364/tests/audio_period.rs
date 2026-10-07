//! Period reload observations from the retained, compiled vAmiga register and
//! reload methods. This isolates digital counter timing from DMA starvation
//! and host audio filtering; it does not assert full-machine phase alignment.

use emu198x_commodore_paula_8364::{AudioField, Paula8364, bits::*};

const DMA: u16 = DMA_MASTER | DMA_AUD0;

fn playing(period: u16) -> Paula8364 {
    let mut paula = Paula8364::new();
    paula.write_audio(0, AudioField::LcLo, 0x1000);
    paula.write_audio(0, AudioField::Len, 8);
    paula.write_audio(0, AudioField::Per, period);
    paula.write_audio(0, AudioField::Vol, 64);
    paula.tick_audio_cck(DMA, None, |_| 0);
    // Discarded startup word, first playback word, then a prefetched word.
    // Deliver outside tick_audio_cck, as the board's retained DMA service does.
    for address in [0x1000, 0x1000, 0x1002] {
        paula.service_audio_dma_word(0, address, false, 0x7f01);
    }
    assert_eq!(
        paula.audio_diagnostic_snapshot().channels[0].output_sample,
        127
    );
    paula
}

fn tick(paula: &mut Paula8364) {
    paula.tick_audio_cck(DMA, None, |_| 0);
}

fn until_sample_changes(paula: &mut Paula8364) -> u32 {
    let previous = paula.audio_diagnostic_snapshot().channels[0].output_sample;
    for elapsed in 1..=65_540 {
        tick(paula);
        if paula.audio_diagnostic_snapshot().channels[0].output_sample != previous {
            return elapsed;
        }
    }
    panic!("no sample transition within one full 16-bit period");
}

#[test]
fn period_reloads_match_compiled_reference_including_zero_and_short_values() {
    let mut observed = Vec::new();
    let mut expected = Vec::new();
    for line in
        include_str!("../../../test-data/commodore/amiga/paula-audio/period-probe/reference.csv")
            .lines()
            .filter(|line| line.starts_with("period,"))
    {
        let values: Vec<u32> = line
            .split(',')
            .skip(1)
            .map(|v| v.parse().expect("reference integer"))
            .collect();
        let period = u16::try_from(values[0]).expect("16-bit register");
        let mut paula = playing(period);
        let elapsed = until_sample_changes(&mut paula);
        eprintln!("period,{period},native={elapsed},reference={}", values[1]);
        observed.push((period, elapsed));
        expected.push((period, values[1]));
    }
    assert_eq!(expected.len(), 11, "all reference periods must run");
    assert_eq!(observed, expected);
}

#[test]
fn period_writes_preserve_current_interval_and_change_the_following_reload() {
    let mut observed = Vec::new();
    let mut expected = Vec::new();
    for line in
        include_str!("../../../test-data/commodore/amiga/paula-audio/period-probe/reference.csv")
            .lines()
            .filter(|line| line.starts_with("write,"))
    {
        let values: Vec<u32> = line
            .split(',')
            .skip(1)
            .map(|v| v.parse().expect("reference integer"))
            .collect();
        let phase = values[0];
        let replacement = u16::try_from(values[1]).expect("16-bit register");
        let mut paula = playing(124);
        for _ in 0..phase {
            tick(&mut paula);
        }
        paula.write_audio(0, AudioField::Per, replacement);
        let first = phase + until_sample_changes(&mut paula);
        let second = first + until_sample_changes(&mut paula);
        eprintln!(
            "write,{phase},{replacement},native={first}/{second},reference={}/{}",
            values[2], values[3]
        );
        observed.push((phase, replacement, first, second));
        expected.push((phase, replacement, values[2], values[3]));
    }
    assert_eq!(expected.len(), 12, "all reference write phases must run");
    assert_eq!(observed, expected);
}
