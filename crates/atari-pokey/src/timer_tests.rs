use super::*;
use std::collections::BTreeSet;

// Captured from Altirra's unchanged RecomputeTimerPeriod. Reproduction and
// source hashes: docs repo plans/2026-10-11-pokey-fast-timer-period/.
#[test]
fn settled_unlinked_periods_match_all_reference_dividers() {
    let mut chip = Pokey::new(1_789_772);
    let mut cases = BTreeSet::new();
    let mut mismatches = Vec::new();
    for line in include_str!("../tests/data/timer-periods.txt").lines() {
        let row: Vec<u32> = line
            .split_whitespace()
            .map(|x| x.parse().expect("reference integer"))
            .collect();
        let [channel, mode, frequency, period] = row.as_slice() else {
            panic!("reference shape")
        };
        assert!(*channel < 4 && *mode < 3 && *frequency < 256 && *period > 0);
        assert!(
            cases.insert((*channel, *mode, *frequency)),
            "duplicate reference row"
        );
        let selected = *channel as usize;
        chip.write(
            0x08,
            match mode {
                0 => 0,
                1 => 1,
                _ => 0x60,
            },
        );
        chip.write((*channel * 2) as u8, *frequency as u8);
        chip.write((*channel * 2 + 1) as u8, 0xaf);
        chip.write(0x09, 0);
        let base = if *mode == 1 { 114 } else { 28 };
        let mut edges = Vec::new();
        for elapsed in 1..=*period * 3 {
            let previous = chip.channels[selected].output;
            chip.tick_channels(elapsed.is_multiple_of(base));
            if chip.channels[selected].output != previous {
                edges.push(elapsed);
                if edges.len() == 3 {
                    break;
                }
            }
        }
        assert_eq!(edges.len(), 3, "missing timer events: {row:?}");
        if edges.windows(2).any(|pair| pair[1] - pair[0] != *period) {
            mismatches.push((row, edges));
        }
    }
    assert_eq!(cases.len(), 3072, "complete channel/mode/divider inventory");
    assert!(
        mismatches.is_empty(),
        "{} period mismatches; first: {:?}",
        mismatches.len(),
        mismatches.first()
    );
}
