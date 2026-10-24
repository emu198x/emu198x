//! Unit checks that the oracle does not make on its own: the ROMs against
//! the formulas they encode, the bus interface and the sample cadence.

use super::*;

/// Run whole internal clocks.
fn run(chip: &mut Ym2413, clocks: u32) {
    for _ in 0..clocks {
        chip.clock();
    }
}

/// Write a register as software must: address, a pause, data, a pause.
fn reg(chip: &mut Ym2413, address: u8, data: u8) {
    chip.write(0, address);
    run(chip, 3);
    chip.write(1, data);
    run(chip, 21);
}

#[test]
fn the_log_sine_rom_is_minus_log2_of_a_quarter_sine() {
    for (i, &entry) in LOGSIN.iter().enumerate() {
        let x = ((i as f64 + 0.5) * std::f64::consts::PI / 512.0).sin();
        let expected = (-x.log2() * 256.0).round() as i32;
        assert!(
            (i32::from(entry) - expected).abs() <= 1,
            "entry {i}: {entry:#x} against {expected:#x}"
        );
    }
}

#[test]
fn the_exponent_rom_is_two_to_the_inverted_fraction() {
    for (i, &entry) in EXP.iter().enumerate() {
        let expected = (2f64.powf((255.0 - i as f64) / 256.0) * 1024.0).round() as i32;
        assert!(
            (i32::from(entry) - expected).abs() <= 2,
            "entry {i}: {entry:#x} against {expected:#x}"
        );
    }
}

#[test]
fn the_prescaler_divides_the_input_clock_by_four() {
    let mut chip = Ym2413::new(Variant::Ym2413);
    for _ in 0..PRESCALER - 1 {
        chip.tick();
    }
    assert_eq!(chip.cycles, 0);
    chip.tick();
    assert_eq!(chip.cycles, 1);
    assert_eq!(chip.prescaler, 0);
}

#[test]
fn a_sample_is_eighteen_internal_clocks() {
    let mut chip = Ym2413::new(Variant::Ym2413);
    run(&mut chip, SLOTS * 5 + 3);
    assert_eq!(chip.take_samples().len(), 5);
    assert!(chip.take_samples().is_empty());
}

#[test]
fn the_user_instrument_registers_read_back_through_the_patch() {
    let mut chip = Ym2413::new(Variant::Ym2413);
    reg(&mut chip, 0x00, 0xf3);
    reg(&mut chip, 0x01, 0x21);
    reg(&mut chip, 0x02, 0x9a);
    reg(&mut chip, 0x03, 0xdd);
    reg(&mut chip, 0x04, 0x5a);
    reg(&mut chip, 0x05, 0xc3);
    reg(&mut chip, 0x06, 0x47);
    reg(&mut chip, 0x07, 0x8e);
    let patch = chip.user_patch();
    assert_eq!(patch.am, [1, 0]);
    assert_eq!(patch.vib, [1, 0]);
    assert_eq!(patch.et, [1, 1]);
    assert_eq!(patch.ksr, [1, 0]);
    assert_eq!(patch.multi, [3, 1]);
    assert_eq!((patch.ksl[0], patch.tl), (2, 0x1a));
    assert_eq!((patch.ksl[1], patch.dc, patch.dm, patch.fb), (3, 1, 1, 5));
    assert_eq!((patch.ar, patch.dr), ([5, 0xc], [0xa, 3]));
    assert_eq!((patch.sl, patch.rr), ([4, 8], [7, 0xe]));
}

#[test]
fn an_address_outside_the_register_map_selects_nothing() {
    let mut chip = Ym2413::new(Variant::Ym2413);
    reg(&mut chip, 0x40, 0xff);
    assert_eq!(chip.user_patch(), Patch::default());
}

/// Set the chip up, then return how far the melody and rhythm output swing
/// across `frames` samples. A silent chip still drives a DC level, so swing
/// rather than size is the measure of sound.
fn play(variant: Variant, setup: &[(u8, u8)], frames: u32) -> (i32, i32) {
    let mut chip = Ym2413::new(variant);
    for &(address, data) in setup {
        reg(&mut chip, address, data);
    }
    chip.take_samples();
    run(&mut chip, SLOTS * frames);
    let samples = chip.take_samples();
    let swing = |pin: usize| {
        let max = samples.iter().map(|s| s[pin]).max().unwrap_or(0);
        let min = samples.iter().map(|s| s[pin]).min().unwrap_or(0);
        max - min
    };
    (swing(0), swing(1))
}

/// Channel 0, instrument 1 at full volume, keyed on at A4-ish.
const NOTE: [(u8, u8); 3] = [(0x30, 0x10), (0x10, 0xac), (0x20, 0x1c)];

#[test]
fn a_keyed_note_sounds_louder_than_a_silent_chip() {
    let (melody, _) = play(Variant::Ym2413, &NOTE, 2000);
    let (quiet, _) = play(Variant::Ym2413, &[], 2000);
    // Muted slots drive the sign bit, so silence still moves by one step.
    assert!(
        quiet <= 1,
        "a silent chip holds a near-steady level: {quiet}"
    );
    assert!(melody > 100, "the note swings {melody}");
}

#[test]
fn rhythm_mode_drives_the_rhythm_pin() {
    let (_, rhythm) = play(Variant::Ym2413, &[(0x0e, 0x3f)], 2000);
    let (_, none) = play(Variant::Ym2413, &[], 2000);
    assert!(none <= 1, "{none}");
    assert!(rhythm > 100, "the drums swing {rhythm}");
}

#[test]
fn the_vrc7_has_no_rhythm_pin_and_keeps_rhythm_mode_on() {
    let mut chip = Ym2413::new(Variant::Vrc7);
    assert!(chip.rhythm_now());
    reg(&mut chip, 0x0e, 0x00);
    assert!(chip.rhythm_now(), "rhythm mode cannot be switched off");
    let (melody, rhythm) = play(Variant::Vrc7, &NOTE, 2000);
    // Six melody slots, and no sign step between them.
    assert!(melody > 100, "the note swings {melody}");
    assert_eq!(rhythm, 0);
}

#[test]
fn reset_keeps_the_variant_and_clears_the_rest() {
    let mut chip = Ym2413::new(Variant::Vrc7);
    reg(&mut chip, 0x00, 0xff);
    run(&mut chip, 7);
    chip.reset();
    assert_eq!(chip.variant, Variant::Vrc7);
    assert_eq!(chip.cycles, 0);
    assert_eq!(chip.user_patch(), Patch::default());
}

#[test]
fn a_snapshot_resumes_identically() {
    let mut chip = Ym2413::new(Variant::Ym2413);
    for (a, d) in [(0x30, 0x30), (0x10, 0x80), (0x20, 0x1d), (0x0e, 0x2a)] {
        reg(&mut chip, a, d);
    }
    run(&mut chip, 1234);
    let bytes = postcard::to_allocvec(&chip).expect("serialises");
    let mut copy: Ym2413 = postcard::from_bytes(&bytes).expect("deserialises");
    run(&mut chip, 5000);
    run(&mut copy, 5000);
    assert_eq!(chip.take_samples(), copy.take_samples());
    assert_eq!((chip.mo, chip.ro), (copy.mo, copy.ro));
}
