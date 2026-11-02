//! Frozen unchanged-Altirra event captures, from the October controller
//! investigation. Its author also wrote the hardware reference: this is
//! executable implementation precedent, not an independent hardware capture.
use emu198x_atari_pokey::Pokey;

fn clock(chip: &mut Pokey, key: u8, top: bool) {
    let address = chip.keyboard_scan_address();
    chip.set_keyboard_returns(key != 0 && ((address >> 1) & 15) == key, top);
    chip.tick_chip();
}

#[test]
fn physical_scanner_matches_all_frozen_reference_events() {
    let mut count = 0;
    for line in include_str!("data/keyboard-scan.csv").lines().skip(1) {
        let values: Vec<u32> = line
            .split(',')
            .map(|s| s.parse().expect("capture integer"))
            .collect();
        assert_eq!(values.len(), 11);
        let mut chip = Pokey::new(1_789_772);
        chip.enable_keyboard_scan();
        chip.write(14, 0xc0);
        chip.write(15, values[2] as u8);
        let selected = values[0] - 1 == values[1];
        let key = if selected { values[3] as u8 } else { 0 };
        let top = selected && values[4] != 0;
        let mut seen = [0u32; 6];
        // The frozen event-only probe starts at an externally supplied edge.
        // SKCTL's real startup edge is 81 clocks after its write; shift the
        // capture origin by 33 to compare the same 192 edges.
        for time in 1..=192 * 114 - 33 {
            clock(&mut chip, key, top);
            if (time + 33) % 114 != 0 {
                continue;
            }
            let status = u32::from(!chip.read(15) & 12);
            let irq = u32::from(!chip.read(14) & 192);
            seen[0] |= status;
            seen[1] |= irq;
            for (index, active) in [(2, status & 4), (3, status & 8), (4, irq & 128)] {
                if seen[index] == 0 && active != 0 {
                    seen[index] = time + 33;
                }
            }
            if irq & 64 != 0 {
                seen[5] += 1;
            }
            chip.write(14, 0);
            chip.write(14, 0xc0);
        }
        assert_eq!(seen.as_slice(), &values[5..], "reference row {line}");
        count += 1;
    }
    assert_eq!(count, 2048);
}

#[test]
fn scanner_keeps_phase_and_shift_while_disabled() {
    for delay in 0..228 {
        let mut chip = Pokey::new(1_789_772);
        chip.enable_keyboard_scan();
        chip.write(15, 1); // leave initialization without enabling scanner
        for _ in 0..delay {
            clock(&mut chip, 0, true);
        }
        chip.write(15, 2);
        let expected = if delay < 81 {
            81 - delay
        } else {
            114 - (delay - 81) % 114
        };
        for _ in 1..expected {
            clock(&mut chip, 0, true);
            assert_eq!(chip.keyboard_scan_address(), 0);
        }
        clock(&mut chip, 0, true);
        assert_eq!(chip.keyboard_scan_address(), 1, "delay {delay}");
    }
    let mut chip = Pokey::new(1_789_772);
    chip.enable_keyboard_scan();
    chip.write(14, 0xc0);
    chip.write(15, 2);
    for _ in 0..81 + 48 * 114 {
        clock(&mut chip, 1, true);
    }
    assert_eq!(chip.read(15) & 8, 0);
    assert_eq!(chip.read(14) & 128, 0);
    let code = chip.read(9);
    chip.write(15, 1);
    for _ in 0..1000 {
        clock(&mut chip, 0, false);
    }
    assert_eq!(chip.keyboard_scan_address(), 0);
    assert_eq!(chip.read(15) & 12, 4); // key released, sampled Shift retained
    assert_eq!(chip.read(9), code);
    chip.write(10, 0); // SKRES must not erase Shift or key state
    assert_eq!(chip.read(15) & 12, 4);
    chip.write(15, 2);
    for _ in 0..17 * 114 {
        clock(&mut chip, 0, false);
    }
    assert_eq!(chip.read(15) & 12, 12);
}

#[test]
fn compact_state_preserves_every_scan_phase_and_rejects_missing_extension_atomically() {
    for phase in 0..114 {
        let mut chip = Pokey::new(1_789_772);
        chip.enable_keyboard_scan();
        chip.write(14, 0xc0);
        chip.write(15, 2);
        for _ in 0..81 + 31 * 114 + phase {
            clock(&mut chip, 15, true);
        }
        let state = chip.save_state();
        let mut restored = Pokey::new(1_789_772);
        assert_eq!(
            restored.load_state(&state).expect("load physical state"),
            state.len()
        );
        assert_eq!(restored.save_state(), state);
        for _ in 0..65 * 114 {
            clock(&mut chip, 15, true);
            clock(&mut restored, 15, true);
            for reg in [9, 14, 15] {
                assert_eq!(restored.read(reg), chip.read(reg));
            }
        }
        let before = restored.save_state();
        assert!(restored.load_state(&state[..63]).is_err());
        assert_eq!(restored.save_state(), before);
        assert!(restored.load_state(&state[..76]).is_err());
        assert_eq!(restored.save_state(), before);
    }
}
