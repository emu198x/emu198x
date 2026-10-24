//! Behaviour a host relies on, case by case.

use super::*;

fn run_samples(dsp: &mut SDsp, samples: usize) -> Vec<(i16, i16)> {
    for _ in 0..samples * usize::from(STEPS_PER_SAMPLE) {
        dsp.tick();
    }
    dsp.take_samples()
}

/// A DSP with one looping square-ish BRR sample in source 0, unmuted.
fn playing() -> SDsp {
    let mut dsp = SDsp::new();
    let block = [0xB3u8, 0x77, 0x77, 0x77, 0x77, 0x99, 0x99, 0x99, 0x99];
    dsp.ram_mut()[0x1000..0x1009].copy_from_slice(&block);
    dsp.ram_mut()[0x0200..0x0204].copy_from_slice(&[0x00, 0x10, 0x00, 0x10]);
    dsp.write(0x5D, 0x02);
    dsp.write(0x6C, 0x20);
    dsp.write(0x0C, 0x7F);
    dsp.write(0x1C, 0x7F);
    dsp.write(0x00, 0x7F);
    dsp.write(0x01, 0x7F);
    dsp.write(0x02, 0x00);
    dsp.write(0x03, 0x10);
    dsp.write(0x05, 0x00);
    dsp.write(0x07, 0x7F); // direct GAIN, full
    dsp
}

#[test]
fn the_gaussian_table_is_the_formula() {
    assert_eq!(gaussian::table_from_formula(), GAUSSIAN);
}

#[test]
fn one_sample_every_32_ticks() {
    let mut dsp = SDsp::new();
    for _ in 0..31 {
        dsp.tick();
    }
    assert!(dsp.take_samples().len() <= 1);
    assert_eq!(run_samples(&mut dsp, 100).len(), 100);
    assert_eq!(dsp.step(), 31);
}

#[test]
fn power_on_is_silent_and_muted() {
    let mut dsp = SDsp::new();
    assert!(run_samples(&mut dsp, 64).iter().all(|s| *s == (0, 0)));
}

#[test]
fn a_keyed_voice_sounds_and_koff_releases_it() {
    let mut dsp = playing();
    dsp.write(0x4C, 0x01);
    let on = run_samples(&mut dsp, 400);
    assert!(on.iter().any(|(l, r)| l.abs() > 1000 && r.abs() > 1000));
    assert!(dsp.read(0x08) > 0, "ENVX reports the envelope");
    dsp.write(0x5C, 0x01);
    run_samples(&mut dsp, 2000);
    assert_eq!(dsp.read(0x08), 0, "released to silence");
}

#[test]
fn mute_silences_the_output_but_not_the_voices() {
    let mut dsp = playing();
    dsp.write(0x4C, 0x01);
    run_samples(&mut dsp, 200);
    dsp.write(0x6C, 0x60);
    let muted = run_samples(&mut dsp, 50);
    assert!(muted.iter().all(|s| *s == (0, 0)));
    assert!(dsp.read(0x08) > 0);
}

#[test]
fn endx_reports_a_finished_sample_and_clears_on_write() {
    let mut dsp = playing();
    // Make the block end without looping.
    dsp.ram_mut()[0x1000] = 0xB1;
    dsp.write(0x4C, 0x01);
    run_samples(&mut dsp, 100);
    assert_eq!(dsp.read(0x7C) & 1, 1);
    dsp.write(0x7C, 0x55);
    assert_eq!(dsp.read(0x7C), 0);
}

#[test]
fn pitch_registers_split_fourteen_bits() {
    let mut dsp = SDsp::new();
    dsp.write(0x12, 0xCD);
    dsp.write(0x13, 0xFF);
    assert_eq!(dsp.voices[1].pitch, 0x3FCD);
    assert_eq!(dsp.read(0x13), 0xFF, "the register keeps what was written");
    dsp.write(0x2D, 0xFF);
    assert!(!dsp.voices[0].modulate, "voice 0 cannot be modulated");
    assert!(dsp.voices[1].modulate);
    dsp.write(0xA0, 0x12);
    assert_eq!(dsp.read(0x20), 0x12, "addresses wrap at 128");
}

#[test]
fn echo_writes_land_in_ram_only_when_enabled() {
    let mut dsp = playing();
    dsp.write(0x6D, 0x80);
    dsp.write(0x7D, 0x01);
    dsp.write(0x4D, 0x01);
    dsp.write(0x4C, 0x01);
    run_samples(&mut dsp, 300);
    assert!(
        dsp.ram()[0x8000..0x8800].iter().all(|b| *b == 0),
        "FLG bit 5 blocks writes"
    );
    dsp.write(0x6C, 0x00);
    run_samples(&mut dsp, 300);
    assert!(dsp.ram()[0x8000..0x8800].iter().any(|b| *b != 0));
}

#[test]
fn a_snapshot_resumes_identically() {
    let mut dsp = playing();
    dsp.write(0x4C, 0x01);
    run_samples(&mut dsp, 37);
    for _ in 0..13 {
        dsp.tick();
    }
    let json = serde_json::to_string(&dsp).expect("serialise");
    let mut copy: SDsp = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(run_samples(&mut dsp, 50), run_samples(&mut copy, 50));
    assert_eq!(SDsp::default().step(), 0);
    assert_eq!(SAMPLE_RATE, 32_000);
}
