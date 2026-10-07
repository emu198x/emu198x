//! CPU-fed startup/holding regression against the original vAmiga schedule.
use emu198x_commodore_paula_8364::{AudioField, Paula8364, bits::*};

fn expected(kind: &str) -> Vec<Vec<i32>> {
    include_str!("../../../test-data/commodore/amiga/paula-audio/interrupt-probe/reference.csv")
        .lines()
        .filter(|line| line.starts_with(kind))
        .map(|line| {
            line.split(',')
                .skip(1)
                .map(|v| v.parse().expect("reference integer"))
                .collect()
        })
        .collect()
}

pub fn main() {
    let reference = expected("manual,");
    assert_eq!(reference.len(), 480);
    let mut rows = 0;
    let mut differences = [0; 5];
    for (scenario, difference_count) in differences.iter_mut().enumerate() {
        for source in 0..4usize {
            let mut p = Paula8364::new();
            p.write_audio(source as u8, AudioField::Per, 8);
            p.write_audio(source as u8, AudioField::Vol, 64);
            let irq = INT_AUD0 << source;
            if scenario == 1 {
                p.write_intreq(INT_SETCLR | irq);
            }
            p.write_audio(source as u8, AudioField::Dat, 0x1122);
            for time in 0..=23 {
                if time != 0 {
                    p.begin_audio_cck();
                    if ((scenario == 2 || scenario == 3) && time == 7)
                        || (scenario == 4 && time == 17)
                    {
                        p.write_intreq(irq);
                    }
                    if (scenario == 2 && time == 12)
                        || (scenario == 3 && time == 4)
                        || (scenario == 4 && time == 20)
                    {
                        p.write_audio(source as u8, AudioField::Dat, 0x3344);
                    }
                    p.finish_audio_cck(0, None, |_| panic!("manual playback requested DMA"));
                }
                let row = vec![
                    source as i32,
                    scenario as i32,
                    0,
                    time,
                    i32::from(p.intreq() & irq != 0),
                    i32::from(p.audio_diagnostic_snapshot().channels[source].output_sample),
                ];
                if row != reference[rows] {
                    *difference_count += 1;
                    eprintln!("manual native={row:?} reference={:?}", reference[rows]);
                }
                rows += 1;
            }
        }
    }
    assert_eq!(rows, reference.len());
    eprintln!("Manual mismatches by scenario: {differences:?}");
    assert_eq!(differences, [0; 5]);
}
