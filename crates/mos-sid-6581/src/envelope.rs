//! SID ADSR envelope generator.
//!
//! Each voice has an independent envelope generator with four phases:
//! Attack, Decay, Sustain, Release. The rate counter controls the speed
//! of each phase. Decay and Release use exponential curves via a
//! period lookup that changes at specific level thresholds.

#![allow(clippy::cast_possible_truncation)]

/// Attack rate counter periods (CPU ticks per step).
/// Index 0 = 2ms, index 15 = 8s. Values from the SID datasheet.
const ATTACK_RATES: [u16; 16] = [
    9, 32, 63, 95, 149, 220, 267, 313, 392, 977, 1954, 3126, 3907, 11_720, 19_532, 31_251,
];

/// Decay/Release rate counter periods (3× attack rate).
const DECAY_RELEASE_RATES: [u16; 16] = [
    9, 32, 63, 95, 149, 220, 267, 313, 392, 977, 1954, 3126, 3907, 11_720, 19_532, 31_251,
];

/// Sustain levels: 4-bit value × 17 gives 0x00..0xFF.
const SUSTAIN_LEVELS: [u8; 16] = [
    0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE,
    0xFF,
];

/// Envelope phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Attack,
    Decay,
    Sustain,
    Release,
}

/// ADSR envelope generator for one SID voice.
pub struct Envelope {
    /// Current envelope output level (0–255).
    pub level: u8,
    /// Current phase.
    pub phase: Phase,
    /// Rate counter (counts down to 0, then steps the level).
    rate_counter: u16,
    /// Exponential counter period (changes with level during decay/release).
    exp_counter: u8,
    /// Exponential counter target period.
    exp_period: u8,
    /// Attack register (4-bit, 0–15).
    pub attack: u8,
    /// Decay register (4-bit, 0–15).
    pub decay: u8,
    /// Sustain register (4-bit, 0–15).
    pub sustain: u8,
    /// Release register (4-bit, 0–15).
    pub release: u8,
    /// Previous gate state (for edge detection).
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

    /// Clock the envelope generator once per CPU tick.
    ///
    /// `gate` is the current state of the gate bit (bit 0 of the voice
    /// control register).
    pub fn clock(&mut self, gate: bool) {
        // Gate edge detection
        if gate && !self.prev_gate {
            // Gate on: enter attack
            self.phase = Phase::Attack;
            self.rate_counter = 0;
            self.exp_counter = 0;
        } else if !gate && self.prev_gate {
            // Gate off: enter release
            self.phase = Phase::Release;
        }
        self.prev_gate = gate;

        // Rate counter
        let rate_period = match self.phase {
            Phase::Attack => ATTACK_RATES[self.attack as usize],
            Phase::Decay => DECAY_RELEASE_RATES[self.decay as usize],
            Phase::Sustain => return, // No counting in sustain
            Phase::Release => DECAY_RELEASE_RATES[self.release as usize],
        };

        self.rate_counter = self.rate_counter.wrapping_add(1);
        if self.rate_counter < rate_period {
            return;
        }
        self.rate_counter = 0;

        // Exponential counter (only for decay/release)
        match self.phase {
            Phase::Attack => {
                // Linear increment
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

    /// Update the exponential period based on the current level.
    ///
    /// The 6581 uses different step periods at specific level thresholds
    /// to approximate an exponential decay curve.
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
