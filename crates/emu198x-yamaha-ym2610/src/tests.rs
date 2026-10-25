//! Chip-level tests: routing, the edges where the YM2610 differs from the
//! OPN2, the status ports and the sample clock. The ADPCM arithmetic is
//! checked against ymfm in `tests/ymfm_oracle.rs`.

#![allow(clippy::unwrap_used)]

use super::*;

/// Enough input clocks for a data write to reach the FM core and its busy
/// flag to clear: 33 internal clocks.
const SETTLE: u32 = 33 * 6;

fn reg(chip: &mut Ym2610, rom: &[u8], bank: u8, address: u8, data: u8) {
    chip.write(bank * 2, address);
    run(chip, rom, 6);
    chip.write(bank * 2 + 1, data);
    run(chip, rom, SETTLE);
}

fn run(chip: &mut Ym2610, rom: &[u8], clocks: u32) {
    for _ in 0..clocks {
        chip.tick(rom, rom);
    }
}

/// A plain tone on FM channel `ch` (0-5): algorithm 7, every operator at
/// full level with an instant attack.
fn fm_tone(chip: &mut Ym2610, ch: u8) {
    let bank = ch / 3;
    let c = ch % 3;
    for op in [0x0, 0x4, 0x8, 0xc] {
        reg(chip, &[], bank, 0x30 + op + c, 0x01);
        reg(chip, &[], bank, 0x40 + op + c, 0x00);
        reg(chip, &[], bank, 0x50 + op + c, 0x1f);
        reg(chip, &[], bank, 0x80 + op + c, 0x0f);
    }
    reg(chip, &[], bank, 0xb0 + c, 0x07);
    reg(chip, &[], bank, 0xb4 + c, 0xc0);
    reg(chip, &[], bank, 0xa4 + c, 0x22);
    reg(chip, &[], bank, 0xa0 + c, 0x69);
    reg(chip, &[], 0, 0x28, 0xf0 | (bank << 2) | c);
}

fn peak(samples: &[Sample]) -> i32 {
    samples
        .iter()
        .map(|s| i32::from(s.fm[0]).abs().max(i32::from(s.fm[1]).abs()))
        .max()
        .unwrap_or(0)
}

#[test]
fn a_sample_takes_144_input_clocks() {
    let mut chip = Ym2610::new();
    run(&mut chip, &[], CLOCKS_PER_SAMPLE * 10);
    assert_eq!(chip.take_samples().len(), 10);
    run(&mut chip, &[], CLOCKS_PER_SAMPLE - 1);
    assert!(chip.take_samples().is_empty());
    run(&mut chip, &[], 1);
    assert_eq!(chip.take_samples().len(), 1);
}

#[test]
fn connected_fm_channels_sound() {
    for ch in [1, 2, 4, 5] {
        let mut chip = Ym2610::new();
        fm_tone(&mut chip, ch);
        chip.take_samples();
        run(&mut chip, &[], CLOCKS_PER_SAMPLE * 200);
        assert!(peak(&chip.take_samples()) > 1000, "channel {ch} is silent");
    }
}

#[test]
fn channels_1_and_4_are_not_connected() {
    for ch in [0, 3] {
        let mut chip = Ym2610::new();
        fm_tone(&mut chip, ch);
        chip.take_samples();
        run(&mut chip, &[], CLOCKS_PER_SAMPLE * 200);
        assert_eq!(peak(&chip.take_samples()), 0, "channel {ch} sounds");
    }
}

#[test]
fn there_is_no_dac() {
    let mut chip = Ym2610::new();
    reg(&mut chip, &[], 1, 0xb6, 0xc0);
    reg(&mut chip, &[], 0, 0x2b, 0x80);
    reg(&mut chip, &[], 0, 0x2a, 0xff);
    chip.take_samples();
    run(&mut chip, &[], CLOCKS_PER_SAMPLE * 20);
    assert_eq!(peak(&chip.take_samples()), 0);
}

#[test]
fn ssg_registers_read_back_and_the_ports_read_ff() {
    let mut chip = Ym2610::new();
    reg(&mut chip, &[], 0, 0x00, 0x5a);
    reg(&mut chip, &[], 0, 0x08, 0x0f);
    chip.write(0, 0x00);
    assert_eq!(chip.read(1), 0x5a);
    chip.write(0, 0x08);
    assert_eq!(chip.read(1), 0x0f);
    chip.write(0, 0x0e);
    assert_eq!(chip.read(1), 0xff);
    chip.write(0, 0xff);
    assert_eq!(chip.read(1), 0x01);
    assert_eq!(chip.read(3), 0);
}

#[test]
fn ssg_tone_reaches_the_ssg_output() {
    let mut chip = Ym2610::new();
    reg(&mut chip, &[], 0, 0x00, 0x40);
    reg(&mut chip, &[], 0, 0x07, 0x3e);
    reg(&mut chip, &[], 0, 0x08, 0x0f);
    chip.take_samples();
    run(&mut chip, &[], CLOCKS_PER_SAMPLE * 500);
    let samples = chip.take_samples();
    let high = samples.iter().filter(|s| s.ssg > 0.1).count();
    let low = samples.iter().filter(|s| s.ssg < 0.01).count();
    assert!(high > 100 && low > 100, "high {high}, low {low}");
}

#[test]
fn a_data_write_through_the_wrong_port_is_ignored() {
    let mut chip = Ym2610::new();
    // Upper address, lower data port: no ADPCM-A write.
    chip.write(2, 0x01);
    chip.write(1, 0x3f);
    assert_eq!(chip.adpcm_a.registers()[1], 0);
    // Lower address, upper data port: no SSG write.
    chip.write(0, 0x00);
    chip.write(3, 0x77);
    assert_eq!(chip.ssg.registers()[0], 0);
}

/// A ROM of loud nibbles, so an ADPCM channel that plays is plainly heard.
fn loud_rom() -> Vec<u8> {
    (0..0x1000u32)
        .map(|i| if i % 2 == 0 { 0x77 } else { 0x7f })
        .collect()
}

#[test]
fn adpcm_a_plays_and_raises_its_end_flag() {
    let rom = loud_rom();
    let mut chip = Ym2610::new();
    reg(&mut chip, &rom, 1, 0x01, 0x3f); // total level: full
    reg(&mut chip, &rom, 1, 0x0a, 0xdf); // channel 2: both sides, full
    reg(&mut chip, &rom, 1, 0x12, 0x00); // start $000000
    reg(&mut chip, &rom, 1, 0x1a, 0x00);
    reg(&mut chip, &rom, 1, 0x22, 0x00); // end $0000FF
    reg(&mut chip, &rom, 1, 0x2a, 0x00);
    chip.take_samples();
    reg(&mut chip, &rom, 1, 0x00, 0x04); // key on channel 2
    assert!(chip.adpcm_a.playing(2));
    // 256 bytes are 512 nibbles at one per three samples.
    run(&mut chip, &rom, CLOCKS_PER_SAMPLE * 3 * 520);
    assert!(peak(&chip.take_samples()) > 1000);
    assert!(!chip.adpcm_a.playing(2));
    assert_eq!(chip.read(2), 0x04);

    // Masking hides the flag; clearing the mask shows it again.
    reg(&mut chip, &rom, 0, 0x1c, 0x00);
    assert_eq!(chip.read(2), 0x04);
    chip.flag_mask = 0;
    assert_eq!(chip.read(2), 0);
    reg(&mut chip, &rom, 0, 0x1c, 0x04);
    assert_eq!(chip.read(2), 0, "writing the bit resets the flag");
    reg(&mut chip, &rom, 0, 0x1c, 0x00);
    assert_eq!(chip.read(2), 0);
}

#[test]
fn adpcm_a_runs_on_every_third_sample() {
    let rom = loud_rom();
    let mut chip = Ym2610::new();
    reg(&mut chip, &rom, 1, 0x01, 0x3f);
    reg(&mut chip, &rom, 1, 0x28, 0x01); // end $0001FF
    reg(&mut chip, &rom, 1, 0x00, 0x01);
    // Each nibble holds for three samples.
    chip.take_samples();
    run(&mut chip, &rom, CLOCKS_PER_SAMPLE * 60);
    let samples = chip.take_samples();
    let changes = samples.windows(2).filter(|w| w[0].fm != w[1].fm).count();
    assert!(
        (18..=21).contains(&changes),
        "{changes} changes in 60 samples"
    );
}

#[test]
fn adpcm_b_plays_and_raises_bit_7() {
    let rom = loud_rom();
    let mut chip = Ym2610::new();
    reg(&mut chip, &rom, 0, 0x11, 0xc0); // both sides
    reg(&mut chip, &rom, 0, 0x12, 0x00); // start $000000
    reg(&mut chip, &rom, 0, 0x13, 0x00);
    reg(&mut chip, &rom, 0, 0x14, 0x00); // end $0000FF
    reg(&mut chip, &rom, 0, 0x15, 0x00);
    reg(&mut chip, &rom, 0, 0x19, 0x00); // delta-N $8000: a nibble per two samples
    reg(&mut chip, &rom, 0, 0x1a, 0x80);
    reg(&mut chip, &rom, 0, 0x1b, 0xff);
    chip.take_samples();
    reg(&mut chip, &rom, 0, 0x10, 0x80);
    assert_ne!(chip.adpcm_b.status() & adpcm::B_STATUS_PLAYING, 0);
    run(&mut chip, &rom, CLOCKS_PER_SAMPLE * 1100);
    assert!(peak(&chip.take_samples()) > 1000);
    assert_eq!(chip.read(2), 0x80);
    // Stopping clears the end flag, and bit 7 follows on the next sample.
    reg(&mut chip, &rom, 0, 0x10, 0x00);
    assert_eq!(chip.read(2), 0);
}

#[test]
fn timer_a_raises_irq() {
    let mut chip = Ym2610::new();
    reg(&mut chip, &[], 0, 0x24, 0xff);
    reg(&mut chip, &[], 0, 0x25, 0x03);
    reg(&mut chip, &[], 0, 0x27, 0x05);
    run(&mut chip, &[], CLOCKS_PER_SAMPLE * 4);
    assert!(chip.irq());
    assert_eq!(chip.read(0) & 0x01, 0x01);
}

#[test]
fn state_survives_a_snapshot() {
    let rom = loud_rom();
    let mut chip = Ym2610::new();
    fm_tone(&mut chip, 2);
    reg(&mut chip, &rom, 1, 0x01, 0x3f);
    reg(&mut chip, &rom, 1, 0x28, 0x01);
    reg(&mut chip, &rom, 1, 0x00, 0x01);
    run(&mut chip, &rom, CLOCKS_PER_SAMPLE * 7 + 5);
    chip.take_samples();

    let bytes = postcard::to_allocvec(&chip).unwrap();
    let mut copy: Ym2610 = postcard::from_bytes(&bytes).unwrap();
    run(&mut chip, &rom, CLOCKS_PER_SAMPLE * 50);
    run(&mut copy, &rom, CLOCKS_PER_SAMPLE * 50);
    assert_eq!(chip.take_samples(), copy.take_samples());
}

#[test]
fn reset_and_defaults_return_to_power_on() {
    let mut chip = Ym2610::default();
    reg(&mut chip, &[], 0, 0x1b, 0x80);
    reg(&mut chip, &[], 1, 0x01, 0x3f);
    // Upper bank past ADPCM-A: only the FM core sees it.
    reg(&mut chip, &[], 1, 0x30, 0x55);
    assert_eq!(chip.adpcm_b.registers()[0x0b], 0x80);
    chip.write(0, 0x20);
    assert_eq!(chip.read(1), 0, "registers past the SSG read zero");
    chip.reset();
    assert_eq!(chip.adpcm_a.registers(), AdpcmA::default().registers());
    assert_eq!(chip.adpcm_b.registers(), AdpcmB::default().registers());
    assert_eq!(chip.adpcm_a.registers()[0x08], 0xdf);
    assert_eq!(chip.adpcm_b.registers()[0x0c], 0xff);
    assert_eq!(chip.address, 0);
}
