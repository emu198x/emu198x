//! SID ADSR envelope generator.

#![allow(clippy::cast_possible_truncation)]

use serde::{Deserialize, Serialize};

// SID envelope rate-counter periods (phi2 clocks per rate-counter step).
// A single table serves attack, decay, AND release — the 6581 uses one rate
// counter for all three phases (reSID `rate_counter_period[]`, VICE 3.10
// src/resid/envelope.cc). These are reSID's periods +1 to absorb our
// increment-then-compare structure.
//
// Decay/release run ~3× slower than attack per the datasheet (rate 0: 2 ms
// attack vs 6 ms decay/release; rate 15: 8 s vs 24 s) — but that slowdown
// comes from the exponential counter (`exp_period` below), NOT from inflating
// the period table. Averaging the exponential divisors (1,2,4,8,16,30 at the
// level thresholds 0x5D/0x36/0x1A/0x0E/0x06) over a full 0xFF→0 sweep gives
// ~757/256 ≈ 2.96×, matching the datasheet ratio. A second, inflated table
// would double-count that slowdown (~9× too slow) and — because it pushes the
// free-running counter far past the attack period — turn the ADSR-delay-bug
// missed-match (see `clock`) into multi-thousand-second silence.
const RATE_COUNTER_PERIODS: [u32; 16] = [
    9, 32, 63, 95, 149, 220, 267, 313, 392, 977, 1954, 3126, 3907, 11_720, 19_532, 31_251,
];

const SUSTAIN_LEVELS: [u8; 16] = [
    0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Envelope {
    pub level: u8,
    pub phase: Phase,
    rate_counter: u32,
    exp_counter: u8,
    exp_period: u8,
    pub attack: u8,
    pub decay: u8,
    pub sustain: u8,
    pub release: u8,
    prev_gate: bool,
}

impl Envelope {
    #[must_use]
    pub fn new() -> Self {
        Self {
            level: 0,
            phase: Phase::Release,
            rate_counter: 0,
            exp_counter: 0,
            exp_period: 1,
            attack: 0,
            decay: 0,
            sustain: 0,
            release: 0,
            prev_gate: false,
        }
    }

    pub fn clock(&mut self, gate: bool) {
        // Transition the phase on gate edges but do NOT reset the
        // rate_counter — the free-running counter is the source of the
        // famous 6581 "ADSR delay bug" that some tunes exploit: when the
        // new phase's rate period is shorter than the old period's
        // remaining count, the comparison skips and the envelope
        // appears to pause until the counter wraps all the way around.
        if gate && !self.prev_gate {
            self.phase = Phase::Attack;
            self.exp_counter = 0;
        } else if !gate && self.prev_gate {
            self.phase = Phase::Release;
        }
        self.prev_gate = gate;

        let rate_period = match self.phase {
            Phase::Attack => RATE_COUNTER_PERIODS[self.attack as usize],
            Phase::Decay => RATE_COUNTER_PERIODS[self.decay as usize],
            Phase::Sustain => return,
            Phase::Release => RATE_COUNTER_PERIODS[self.release as usize],
        };

        // The rate counter is 15-bit on real hardware, so it wraps at 0x8000.
        // Equality check (not `<`): the counter must match the period exactly,
        // so if a gate edge switches to a phase whose period sits below the
        // counter's current value, the match is missed and the counter must
        // wrap before triggering again (the 6581 "ADSR delay bug" — the
        // counter is never reset on the edge). Bounding the wrap to 15 bits
        // caps that delay at ~0x8000 cycles (~33 ms), exactly as reSID does
        // (`++rate_counter & 0x8000`, VICE 3.10 src/resid/envelope.h) — a u32
        // wrap would stretch the same miss to ~4000 s of silence.
        self.rate_counter = self.rate_counter.wrapping_add(1) & 0x7FFF;
        if self.rate_counter != rate_period {
            return;
        }
        self.rate_counter = 0;

        match self.phase {
            Phase::Attack => {
                self.level = self.level.saturating_add(1);
                if self.level == 0xFF {
                    self.phase = Phase::Decay;
                    self.rate_counter = 0;
                }
                self.update_exp_period();
            }
            Phase::Decay => {
                self.exp_counter = self.exp_counter.wrapping_add(1);
                if self.exp_counter < self.exp_period {
                    return;
                }
                self.exp_counter = 0;

                let sustain_level = SUSTAIN_LEVELS[self.sustain as usize];
                if self.level > sustain_level {
                    self.level = self.level.saturating_sub(1);
                    self.update_exp_period();
                }
                if self.level <= sustain_level {
                    self.level = sustain_level;
                    self.phase = Phase::Sustain;
                }
            }
            Phase::Sustain => {}
            Phase::Release => {
                self.exp_counter = self.exp_counter.wrapping_add(1);
                if self.exp_counter < self.exp_period {
                    return;
                }
                self.exp_counter = 0;

                if self.level > 0 {
                    self.level = self.level.saturating_sub(1);
                    self.update_exp_period();
                }
            }
        }
    }

    fn update_exp_period(&mut self) {
        self.exp_period = if self.level >= 0x5D {
            1
        } else if self.level >= 0x36 {
            2
        } else if self.level >= 0x1A {
            4
        } else if self.level >= 0x0E {
            8
        } else if self.level >= 0x06 {
            16
        } else {
            30
        };
    }
}

impl Default for Envelope {
    fn default() -> Self {
        Self::new()
    }
}
