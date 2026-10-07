//! Known-gap diagnostic, not a claim that the Minimig pulse phase is hardware truth.
//! Exits unsuccessfully while native scalar output differs from the PWM corpus.

use emu198x_commodore_paula_8364::{AudioField, Paula8364};
use std::{error::Error, fs, io};

fn main() -> Result<(), Box<dyn Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("expected uncompressed reference CSV")?;
    let input = fs::read_to_string(path)?;
    let mut p = Paula8364::new();
    let mut full = 0.0;
    let mut observations = 0;
    let mut disagreements = 0;
    for line in input.lines() {
        let row = line
            .split(',')
            .map(str::parse::<u16>)
            .collect::<Result<Vec<_>, _>>()?;
        if row.len() != 8 {
            return Err("expected eight columns".into());
        }
        if row[3] == 0 {
            p = Paula8364::new();
            p.write_audio(0, AudioField::Per, 1000);
            p.write_audio(0, AudioField::Vol, 64);
            p.write_audio(0, AudioField::Dat, 0x4040);
            for _ in 0..8 {
                if p.audio_diagnostic_snapshot().channels[0].output_sample == 64 {
                    break;
                }
                p.tick_audio_cck(0, None, |_| panic!("manual playback requested DMA"));
            }
            full = p.mix_audio_stereo().1;
            if full <= 0.0 {
                return Err("manual startup produced no audible sample".into());
            }
            p.write_audio(0, AudioField::Vol, if row[0] == 0 { row[1] } else { 32 });
        }
        if row[0] != 0 && row[3] == row[2] {
            p.write_audio(0, AudioField::Vol, row[1]);
        }
        let sample = p.audio_diagnostic_snapshot().channels[0].output_sample;
        if sample != 64 {
            return Err("constant raw sample changed".into());
        }
        // Normalize through native full-volume output, preserving its DAC curve.
        let output = p.mix_audio_stereo().1 / full * 64.0;
        disagreements += usize::from((output - f32::from(row[7])).abs() > 0.001);
        observations += 1;
        p.tick_audio_cck(0, None, |_| panic!("manual playback requested DMA"));
    }
    if observations != 36_864 {
        return Err("incorrect observation inventory".into());
    }
    println!("{{\"observations\":{observations},\"pwm_disagreements\":{disagreements}}}");
    if disagreements != 0 {
        return Err(io::Error::other("native scalar output differs from PWM reference").into());
    }
    Ok(())
}
