//! CPU clock divider in native oscillator periods. GCC1702B page 29 extends
//! phase 2 and the following phase 1 for TIA/RIOT accesses. Independent phase
//! traces and the input-latch boundary are recorded in the docs repository's
//! `plans/2026-10-10-maria-clock-handoff.md`.

use serde::{Deserialize, Serialize};

use super::Maria;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Clock {
    pub remaining: u8,
    pub phase2: bool,
    pub selected_slow: bool,
    pub held_slow: bool,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            remaining: 4,
            phase2: false,
            selected_slow: false,
            held_slow: false,
        }
    }
}

impl Clock {
    fn tick(&mut self, slow: bool) -> (bool, bool) {
        self.remaining -= 1;
        if !self.phase2 && self.remaining == 1 {
            // The divider selects phase 2 one native tick before its strobe.
            self.selected_slow = slow;
        }
        if self.remaining != 0 {
            return (false, false);
        }
        self.phase2 = !self.phase2;
        let extend = if self.phase2 {
            // Retain the access at the phase-2 latch for the following phase 1.
            self.held_slow = slow;
            self.selected_slow
        } else {
            self.held_slow
        };
        self.remaining = if extend { 6 } else { 4 };
        (!self.phase2, self.phase2)
    }
}

impl Maria {
    /// Advance the CPU clock divider by one native oscillator period, sampling
    /// `address_in` and producing one-tick `phi1` / `phi2` strobes.
    ///
    /// This clocks the divider only. The scanline compatibility driver still
    /// owns DMA/rendering until the complete native DMA pipeline is connected.
    /// MARIA is enabled; MEN/reset transitions are not modelled by this entry.
    pub fn tick_clock(&mut self) {
        let address = self.address_in;
        let tia = address & 0xfce0 == 0;
        let riot = matches!(address & 0xfe80, 0x0280 | 0x0480);
        (self.phi1, self.phi2) = self.clock.tick((tia || riot) && !self.dma.slow_inhibit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MariaRegion;

    #[test]
    fn strobes_match_all_108_measured_clock_vectors() {
        let mut cases = std::collections::BTreeSet::new();
        for line in include_str!("../tests/data/clock-vectors.txt").lines() {
            let (name, trace) = line.split_once(' ').expect("vector");
            let fields: Vec<_> = name.split('-').collect();
            assert_eq!(fields.len(), 4);
            let mode: u8 = fields[0][1..].parse().expect("mode");
            let pal: u8 = fields[1][1..].parse().expect("region");
            let address = u16::from_str_radix(&fields[2][1..], 16).expect("address");
            let offset: u16 = fields[3][1..].parse().expect("offset");
            assert!(cases.insert((mode, pal, address, offset)), "duplicate case");
            let expected: Vec<(u16, u8)> = trace
                .split(',')
                .map(|entry| {
                    let (tick, phase) = entry.split_once(':').expect("event");
                    (tick.parse().expect("tick"), phase.parse().expect("phase"))
                })
                .collect();
            let mut chip = Maria::new(if pal == 0 {
                MariaRegion::Ntsc
            } else {
                MariaRegion::Pal
            });
            let mut phase_count = 0;
            let mut actual = Vec::new();
            // The capture begins at 256. Its preceding phase 1 was at 254;
            // normalize only the initial phase, never any subsequent interval.
            for tick in 255..1024 {
                let slow = tick >= 256
                    && match mode {
                        0 => false,
                        1 => true,
                        2 => tick >= 512 + offset,
                        3 => tick < 512 + offset,
                        4 => (phase_count / 3) % 2 == 1,
                        _ => panic!("unknown mode"),
                    };
                chip.address_in = if slow { address } else { 0x8000 };
                chip.tick_clock();
                if chip.phi1 {
                    phase_count += 1;
                    actual.push((tick, 1));
                } else if chip.phi2 {
                    actual.push((tick, 2));
                }
            }
            assert_eq!(actual, expected, "{name}");
        }
        let mut inventory = std::collections::BTreeSet::new();
        for mode in 0..5 {
            for pal in 0..2 {
                for address in [0, 0x0280] {
                    for offset in 0..if matches!(mode, 2 | 3) { 12 } else { 1 } {
                        inventory.insert((mode, pal, address, offset));
                    }
                }
            }
        }
        assert_eq!(cases, inventory);
    }

    #[test]
    fn direct_snapshots_resume_every_divider_phase_and_input_latch() {
        for region in [MariaRegion::Ntsc, MariaRegion::Pal] {
            for elapsed in 1..=48 {
                let mut chip = Maria::new(region);
                for tick in 0..elapsed {
                    chip.address_in = if tick % 17 < 8 { 0x0280 } else { 0x8000 };
                    chip.tick_clock();
                }
                let saved = chip.save_state();
                let mut restored = Maria::new(region);
                restored.load_state(&saved).expect("clock snapshot");
                assert_eq!(
                    (restored.address_in, restored.phi1, restored.phi2),
                    (chip.address_in, chip.phi1, chip.phi2)
                );
                assert_eq!(restored.clock, chip.clock);
                for tick in elapsed..elapsed + 100 {
                    let address = if tick % 17 < 8 { 0x0280 } else { 0x8000 };
                    chip.address_in = address;
                    restored.address_in = address;
                    chip.tick_clock();
                    restored.tick_clock();
                    assert_eq!(
                        (restored.phi1, restored.phi2),
                        (chip.phi1, chip.phi2),
                        "elapsed {elapsed}, tick {tick}"
                    );
                    assert_eq!(restored.clock, chip.clock);
                }
            }
        }
    }
}
