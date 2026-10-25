//! Saved phases of Agnus's display request generator, clocked by its CCK edge.
//!
//! Request generation precedes comparators. These phases carry across horizontal
//! wrap; neither an endpoint calculation nor a delayed copy of a slot grid can
//! represent them. Source observations: display-dma-sequencer reference corpus.

use serde::{Deserialize, Serialize};

use crate::{DisplayDmaChannel, DisplayDmaReservation};

/// Inputs already decoded by the installed Agnus revision for this CCK.
#[derive(Clone, Copy, Debug)]
pub struct DisplayDmaInputs {
    pub hpos: u16,
    pub clock: bool,
    pub enhanced: bool,
    pub alice: bool,
    pub dma: bool,
    pub vertical: bool,
    pub hard_limit_disabled: bool,
    pub start: u16,
    pub stop: u16,
    pub fetch_unit: u8,
    pub fetch_start: u8,
    pub max_planes: u8,
    pub planes: u8,
    pub width_words: u8,
    pub fmode: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayDmaSequencer {
    // Same four physical states as registered BPRUN: idle, pending enable,
    // running, and pending disable. Positive states still generate requests.
    run: i8,
    cycle: u8,
    stopping: u8,
    soft_enable: i8,
    hard_closed: bool,
    previous_enable: bool,
}

impl DisplayDmaSequencer {
    /// Diagnostic phase values; the cycle is the five-bit fetch-unit phase.
    #[must_use]
    pub const fn phases(&self) -> (i8, u8, u8, i8, bool, bool) {
        (
            self.run,
            self.cycle,
            self.stopping,
            self.soft_enable,
            self.hard_closed,
            self.previous_enable,
        )
    }

    pub fn validate(&self) -> Result<(), String> {
        if !matches!(self.run, -1..=2)
            || self.cycle >= 32
            || self.stopping > 2
            || !matches!(self.soft_enable, -1..=1)
        {
            return Err("invalid saved display DMA sequencer phase".into());
        }
        Ok(())
    }

    /// Generate this CCK's reservation, then clock its comparator transitions.
    /// No RAM access, bus arbitration, additional clock or beam update occurs.
    pub fn tick(&mut self, input: DisplayDmaInputs) -> Option<DisplayDmaReservation> {
        assert!(matches!(input.fetch_unit, 8 | 16 | 32));
        assert!(matches!(input.fetch_start, 2 | 4 | 8 | 16 | 32));
        assert!(matches!(input.max_planes, 2 | 4 | 8));
        assert!(input.planes <= input.max_planes);
        let mut request = None;
        if self.run > 0 {
            let last_mask = if input.alice && self.stopping == 2 {
                7
            } else {
                input.fetch_unit - 1
            };
            let last = self.cycle & last_mask == last_mask;
            let position = usize::from(self.cycle & (input.fetch_start - 1));
            if input.dma {
                const EIGHT: [u8; 8] = [7, 3, 5, 1, 6, 2, 4, 0];
                const FOUR: [u8; 4] = [3, 1, 2, 0];
                const TWO: [u8; 2] = [1, 0];
                let sequence: &[u8] = match input.max_planes {
                    2 => &TWO,
                    4 => &FOUR,
                    _ => &EIGHT,
                };
                if let Some(&plane) = sequence.get(position)
                    && plane < input.planes
                {
                    let cycle = self.cycle & 7;
                    let add_modulo = self.stopping == 2
                        && (input.max_planes == 8
                            || input.max_planes == 4 && cycle >= 4
                            || input.max_planes == 2 && cycle >= 6);
                    request = Some(DisplayDmaReservation {
                        channel: DisplayDmaChannel::Bitplane(plane),
                        width_words: input.width_words,
                        fmode: input.fmode,
                        add_modulo,
                    });
                }
            }
            if input.clock {
                self.cycle = (self.cycle + 1) & 31;
            }
            if last {
                if self.stopping == 2 {
                    self.stopping = 0;
                    self.run = 0;
                    if !input.enhanced {
                        self.hard_closed = true;
                    }
                }
                if self.stopping == 1 {
                    self.stopping = 2;
                }
            }
        }

        if input.enhanced {
            self.enhanced_comparators(input);
        } else {
            self.original_comparators(input);
        }
        request
    }

    fn latch_start(&mut self, hpos: u16) {
        if self.run < 0 && !hpos.is_multiple_of(2) {
            self.run = 1;
            self.cycle = 0;
        }
    }

    fn enhanced_comparators(&mut self, input: DisplayDmaInputs) {
        self.latch_start(input.hpos);
        if self.soft_enable < 0 {
            self.soft_enable = 0;
            if self.run != 0 && self.stopping == 0 {
                self.stopping = 1;
            }
        }
        if input.hpos == 0x18 {
            self.hard_closed = false;
        }
        if input.hpos == input.start {
            self.soft_enable = 1;
        }
        if input.hpos == 0xd7 {
            self.hard_closed = true;
            if self.run != 0 && self.stopping == 0 && !input.hard_limit_disabled {
                self.stopping = 1;
            }
        }
        if input.hpos == input.stop {
            if self.run != 0 && self.stopping == 0 {
                self.stopping = 1;
            }
            if input.stop != input.start {
                self.soft_enable = if self.soft_enable != 0 { -1 } else { 0 };
            }
        }
        if input.hpos.is_multiple_of(2) {
            let enable = input.dma
                && input.vertical
                && self.soft_enable > 0
                && (!self.hard_closed || input.hard_limit_disabled);
            if self.run == 0 && enable && !self.previous_enable {
                self.run = -1;
            }
            self.previous_enable = enable;
        }
        if self.run == 2 {
            self.run = 0;
            self.stopping = match self.stopping {
                0 => 1,
                1 => 2,
                other => other,
            };
        }
        self.disable(input);
    }

    fn original_comparators(&mut self, input: DisplayDmaInputs) {
        // Stop observes the run before a coincident pending start latches on.
        if input.hpos == input.stop && self.run > 0 && self.stopping == 0 {
            self.stopping = 1;
        }
        self.latch_start(input.hpos);
        if input.hpos == 0x18 {
            self.hard_closed = false;
        }
        if input.hpos == 0xd7 && self.run != 0 && self.stopping == 0 {
            self.stopping = 1;
        }
        if !self.hard_closed
            && input.hpos == input.start
            && self.run == 0
            && input.dma
            && input.vertical
        {
            self.run = -1;
        }
        if self.run == 2 {
            if self.stopping == 1 {
                self.stopping = 2;
            }
            self.run = 0;
        }
        self.disable(input);
    }

    fn disable(&mut self, input: DisplayDmaInputs) {
        if (!input.dma || !input.vertical) && self.run == 1 {
            self.run = 2;
            if self.stopping == 1 {
                self.stopping = 2;
                self.run = 0;
            }
        }
    }
}
