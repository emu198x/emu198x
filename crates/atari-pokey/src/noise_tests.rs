use super::*;

#[test]
fn blocked_noise_clocks_preserve_timer_irqs_in_linked_and_unlinked_modes() {
    for audctl in [0, 0x10, 0x08, 0x18, 0x78] {
        let mut chip = Pokey::default();
        chip.write(0x08, audctl);
        chip.write(0x0e, 7);
        chip.poly5_table[0] = 0;
        for channel in &mut chip.channels {
            channel.counter = 0;
            channel.audc = 0x2f; // Poly-5 gates the toggle clock.
            channel.output = true;
        }
        chip.tick_channels(true);
        assert!(chip.channels.iter().all(|ch| ch.output));
        assert_eq!(chip.read(0x0e) & 7, 0, "audio gating must not gate IRQs");
        assert!(chip.irq_pending());
    }
}

#[test]
fn linked_pairs_clock_both_retained_waveforms() {
    let mut chip = Pokey::default();
    chip.write(0x08, 0x18);
    chip.poly4_table[0] = 1;
    for channel in &mut chip.channels {
        channel.counter = 0;
        channel.audc = 0xcf;
    }
    chip.tick_channels(true);
    assert!(chip.channels.iter().all(|ch| ch.output));
    // A low-byte borrow without a full underflow clocks only the low channel.
    chip.poly4_table[0] = 0;
    chip.channels[0].counter = 0x100;
    chip.channels[2].counter = 0x100;
    chip.tick_channels(true);
    assert!(!chip.channels[0].output);
    assert!(chip.channels[1].output);
    assert!(!chip.channels[2].output);
    assert!(chip.channels[3].output);
}

/// Observations from Altirra's compiled FireTimer template, not a duplicate
/// expected-value expression. Synthetic polynomial inputs isolate this stage.
#[test]
fn noise_flipflop_matches_compiled_reference_transfers() {
    let mut chip = Pokey::default();
    let mut rows = 0;
    let mut mismatches = 0;
    let mut seen = [false; 4096];
    for line in include_str!("../tests/data/noise-transfer.txt").lines() {
        if line.starts_with('#') {
            continue;
        }
        let values: Vec<u8> = line
            .split_whitespace()
            .map(|x| x.parse().expect("vector byte"))
            .collect();
        assert_eq!(values.len(), 8);
        let [
            channel,
            mode,
            poly9,
            bits,
            previous,
            fire,
            expected,
            _events,
        ] = values[..]
        else {
            unreachable!()
        };
        let channel = usize::from(channel);
        assert!(channel < 4 && mode < 8 && poly9 < 2 && bits < 16);
        assert!(previous < 2 && fire < 2 && expected < 2);
        let key = (((((channel * 8 + usize::from(mode)) * 2 + usize::from(poly9)) * 16
            + usize::from(bits))
            * 2
            + usize::from(previous))
            * 2)
            + usize::from(fire);
        assert!(!seen[key], "duplicate reference input row");
        seen[key] = true;
        for volume_only in [0, 0x10] {
            chip.audctl = if poly9 != 0 { 0x80 } else { 0 };
            chip.poly_counter = 0;
            chip.poly4_table[0] = (bits >> 3) & 1;
            chip.poly5_table[0] = (bits >> 2) & 1;
            chip.poly9_table[0] = (bits >> 1) & 1;
            chip.poly17_table[0] = bits & 1;
            for ch in &mut chip.channels {
                *ch = Channel::new();
                ch.counter = 1;
            }
            chip.channels[channel].counter = if fire != 0 { 0 } else { 1 };
            chip.channels[channel].audc = mode << 5 | volume_only | 15;
            chip.channels[channel].output = previous != 0;
            chip.tick_channels(true);
            if u8::from(chip.channels[channel].output) != expected {
                mismatches += 1;
            }
            if volume_only != 0 {
                assert_eq!(chip.mix_with_channels().1[channel], 1.0);
            }
        }
        rows += 1;
    }
    assert_eq!(rows, 4096);
    assert_eq!(
        mismatches, 0,
        "noise transfer mismatches across 8192 visible/hidden cases"
    );
}
