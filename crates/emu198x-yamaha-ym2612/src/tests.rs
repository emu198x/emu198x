use super::*;

/// Write a register the way a driver does: address, then data, then wait
/// out the busy flag.
fn reg(chip: &mut Ym2612, part: u8, addr: u8, data: u8) {
    chip.write(part * 2, addr);
    chip.clock();
    chip.write(part * 2 + 1, data);
    for _ in 0..32 {
        chip.clock();
    }
}

/// Channel 1 playing a sine on operator 4.
fn sine(variant: Variant) -> Ym2612 {
    let mut chip = Ym2612::new(variant);
    reg(&mut chip, 0, 0xB0, 0x07);
    for op in [0x40, 0x44, 0x48] {
        reg(&mut chip, 0, op, 0x7F);
    }
    reg(&mut chip, 0, 0x4C, 0x00);
    reg(&mut chip, 0, 0x5C, 0x1F);
    reg(&mut chip, 0, 0xA4, 0x22);
    reg(&mut chip, 0, 0xA0, 0x69);
    reg(&mut chip, 0, 0x28, 0x80);
    chip
}

#[test]
fn six_input_clocks_make_one_internal_clock() {
    let mut chip = Ym2612::new(Variant::Ym3438);
    for _ in 0..5 {
        chip.tick();
    }
    assert_eq!(
        chip.cycles, 0,
        "five input clocks do not reach the pipeline"
    );
    chip.tick();
    assert_eq!(chip.cycles, 1);
    for _ in 0..PRESCALER as usize * 23 {
        chip.tick();
    }
    assert_eq!(chip.cycles, 0, "144 input clocks are one whole sample");
}

#[test]
fn one_sample_comes_out_per_24_internal_clocks() {
    let mut chip = sine(Variant::Ym3438);
    chip.take_samples();
    for _ in 0..24 * 100 {
        chip.clock();
    }
    let samples = chip.take_samples();
    assert_eq!(samples.len(), 100);
    assert!(
        samples.iter().any(|s| s[0] != 0),
        "the sine reaches the left pin"
    );
    assert_eq!(
        samples.iter().map(|s| s[0]).collect::<Vec<_>>(),
        samples.iter().map(|s| s[1]).collect::<Vec<_>>(),
        "panned centre"
    );
    assert!(chip.take_samples().is_empty(), "taking empties the buffer");
}

#[test]
fn the_ym2612_ladder_lifts_silence_off_zero() {
    // With every channel silent the YM3438 outputs exactly zero, while the
    // YM2612's DAC still steps between the sign levels of six channels.
    let mut cmos = Ym2612::new(Variant::Ym3438);
    let mut nmos = Ym2612::new(Variant::Ym2612);
    for _ in 0..24 * 10 {
        cmos.clock();
        nmos.clock();
    }
    assert!(cmos.take_samples().iter().all(|s| *s == [0, 0]));
    assert!(nmos.take_samples().iter().all(|s| s[0] > 0 && s[1] > 0));
}

#[test]
fn a_data_write_holds_busy_for_32_clocks() {
    let mut chip = Ym2612::new(Variant::Ym3438);
    chip.write(0, 0x30);
    chip.clock();
    chip.write(1, 0x01);
    chip.clock();
    chip.clock();
    assert_eq!(chip.read(0) & 0x80, 0x80, "busy after the write lands");
    for _ in 0..32 {
        chip.clock();
    }
    assert_eq!(chip.read(0) & 0x80, 0, "and clear 32 clocks later");
}

#[test]
fn timer_a_overflows_and_raises_irq() {
    let mut chip = Ym2612::new(Variant::Ym3438);
    // Timer A at its shortest period, loaded and enabled.
    reg(&mut chip, 0, 0x24, 0xFF);
    reg(&mut chip, 0, 0x25, 0x03);
    reg(&mut chip, 0, 0x27, 0x05);
    for _ in 0..24 * 4 {
        chip.clock();
    }
    assert!(chip.irq());
    assert_eq!(chip.read(0) & 1, 1);
    // Reset the flag and stop the timer, or it overflows again within the
    // 32 clocks the write takes.
    reg(&mut chip, 0, 0x27, 0x10);
    assert!(!chip.irq(), "the reset bit clears the flag");
}

#[test]
fn only_port_0_drives_the_status_on_a_discrete_ym2612() {
    let mut chip = Ym2612::new(Variant::Ym2612);
    assert!(!chip.status_on_all_ports);
    chip.write(0, 0x30);
    chip.clock();
    chip.write(1, 0x01);
    chip.clock();
    chip.clock();
    assert_eq!(chip.read(1), 0, "nothing on the bus before any status read");
    assert_eq!(chip.read(0), 0x80);
    assert_eq!(chip.read(2), 0x80, "port 2 shows what the bus still holds");
    for _ in 0..STATUS_HOLD_YM2612 {
        chip.clock();
    }
    assert_eq!(chip.read(2), 0, "until it decays");
}

#[test]
fn reset_keeps_the_variant_and_read_wiring() {
    let mut chip = sine(Variant::Ym2612);
    chip.status_on_all_ports = true;
    chip.reset();
    assert_eq!(chip.variant, Variant::Ym2612);
    assert!(chip.status_on_all_ports);
    assert_eq!(chip.cycles, 0);
    assert_eq!(
        chip.eg_level, [0x3ff; SLOTS],
        "every envelope back at silence"
    );
}

#[test]
fn a_snapshot_resumes_clock_for_clock() {
    let mut chip = sine(Variant::Ym2612);
    for _ in 0..1000 {
        chip.clock();
    }
    let bytes = postcard::to_allocvec(&chip).expect("encode");
    let mut restored: Ym2612 = postcard::from_bytes(&bytes).expect("decode");
    for _ in 0..5000 {
        chip.clock();
        restored.clock();
        assert_eq!((chip.mol, chip.mor), (restored.mol, restored.mor));
    }
}

#[test]
fn the_test_pin_marks_the_last_slot_when_enabled() {
    let mut chip = Ym2612::new(Variant::Ym3438);
    reg(&mut chip, 0, 0x2C, 0x80);
    let mut high = Vec::new();
    for _ in 0..48 {
        if chip.test_pin() {
            high.push(chip.cycles);
        }
        chip.clock();
    }
    assert_eq!(high, vec![23, 23]);
}
