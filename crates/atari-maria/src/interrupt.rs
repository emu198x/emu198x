//! Sampled interrupt controller and output latch recovered from GCC's MARIA
//! layout. See the docs repository's `2026-10-12-maria-interrupt-completion`
//! evidence. CPU clock edges advance the request; no wall-time pulse timer.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub(super) enum Phase {
    #[default]
    Idle,
    AwaitRelease,
    AwaitEdge,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub(super) enum Command {
    #[default]
    Hold,
    Set,
    Clear,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Interrupt {
    pub phase: Phase,
    pub sampled_phase: Phase,
    pub command: Command,
    pub sampled_command: Command,
    pub request: bool,
}

impl Interrupt {
    pub fn tick(&mut self, input_phase: bool, edge: bool, halt: bool, nmi: &mut bool) {
        if !input_phase {
            self.phase = self.sampled_phase;
            self.command = self.sampled_command;
            return;
        }
        // The output latch captures the previous output bank before the input
        // bank samples this cycle's controller state and external signals.
        match self.command {
            Command::Hold => {}
            Command::Set => *nmi = true,
            Command::Clear => *nmi = false,
        }
        (self.sampled_phase, self.sampled_command) = match self.phase {
            Phase::Idle if edge => (
                if halt {
                    Phase::AwaitRelease
                } else {
                    Phase::Idle
                },
                Command::Clear,
            ),
            Phase::AwaitRelease if !halt && !self.request => (Phase::Idle, Command::Hold),
            Phase::AwaitRelease if !halt && edge => (Phase::AwaitEdge, Command::Hold),
            Phase::AwaitEdge if edge => (Phase::Idle, Command::Set),
            phase => (phase, Command::Hold),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Maria, MariaRegion};

    #[test]
    fn sampled_stages_match_all_32832_circuit_cases() {
        let mut cases = 0;
        let mut pulses = 0;
        let mut vectors = 0;
        for line in include_str!("../tests/data/interrupt-vectors.txt").lines() {
            let (weight, encoded) = line.split_once(' ').expect("circuit vector");
            let weight: usize = weight.parse().expect("case multiplicity");
            assert!(weight > 0);
            assert_eq!(encoded.len(), 239 * 2);
            let mut stage = Interrupt::default();
            let mut nmi = false;
            for index in 0..239 {
                let sample = u8::from_str_radix(&encoded[index * 2..index * 2 + 2], 16)
                    .expect("phase sample");
                let old_nmi = nmi;
                stage.request = sample & 8 != 0;
                stage.tick(sample & 1 != 0, sample & 2 != 0, sample & 4 != 0, &mut nmi);
                assert_eq!(
                    nmi,
                    sample & 16 != 0,
                    "vector {vectors}, step {}",
                    index + 1
                );
                pulses += usize::from(nmi && !old_nmi) * weight;
            }
            assert!(!nmi, "pulse must release");
            cases += weight;
            vectors += 1;
        }
        assert_eq!(vectors, 300);
        assert_eq!(cases, 32832);
        assert_eq!(pulses, 16416);
    }

    #[test]
    fn direct_snapshots_preserve_every_interrupt_stage() {
        let mut cases = 0;
        let mut asserted = 0;
        for region in [MariaRegion::Ntsc, MariaRegion::Pal] {
            for slow in [false, true] {
                let mut chip = Maria::new(region);
                chip.write(0x0c, 0x18);
                chip.write(0x10, 0);
                chip.write(0x1c, 0x40);
                let mut memory = [0; 65536];
                for entry in memory[0x1800..0x1c00].as_chunks_mut::<3>().0 {
                    entry.copy_from_slice(&[0x80, 0x1c, 0]);
                }
                let advance = |chip: &mut Maria| {
                    chip.address_in = if chip.dma_drive {
                        chip.dma_address
                    } else if slow {
                        0x0280
                    } else {
                        0x8000
                    };
                    chip.dma_data_in = memory[usize::from(chip.dma_address)];
                    chip.tick_dma();
                };
                for _ in 0..14500 {
                    advance(&mut chip);
                }
                for _ in 0..160 {
                    advance(&mut chip);
                    asserted += usize::from(chip.nmi);
                    let saved = chip.save_state();
                    let mut restored = Maria::new(region);
                    assert_eq!(
                        restored.load_state(&saved).expect("interrupt restore"),
                        saved.len()
                    );
                    assert_eq!(restored.interrupt, chip.interrupt);
                    for _ in 0..120 {
                        advance(&mut chip);
                        advance(&mut restored);
                        assert_eq!(restored.nmi, chip.nmi);
                        assert_eq!(restored.interrupt, chip.interrupt);
                        assert_eq!((restored.phi1, restored.phi2), (chip.phi1, chip.phi2));
                    }
                    assert_eq!(restored.save_state(), chip.save_state());
                    chip.load_state(&saved).expect("rewind fixture");
                    cases += 1;
                }
            }
        }
        assert_eq!(cases, 640);
        assert!(asserted >= 16, "must save active pulse state");
    }
}
