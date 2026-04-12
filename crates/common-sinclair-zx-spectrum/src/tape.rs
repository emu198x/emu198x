//! Pulse-driven tape playback support for Spectrum-family machines.
//!
//! Source references:
//! - `docs/systems/spectrum.md`
//! - Adapted from `/Users/stevehill/Projects/Emu198x-Older/crates/common-sinclair-zx-spectrum/src/tape.rs`
//!
//! Standard ROM-speed TAP blocks and arbitrary TZX pulse streams both reduce
//! to the same machine-facing form: a sequence of pulse lengths in CPU
//! T-states. The player below advances on the real 3.5 MHz T-state cadence and
//! exposes the current EAR level to machine-local `$FE` read logic.

/// Standard ROM tape pilot pulse length in T-states.
pub const PILOT_PULSE: u32 = 2_168;

/// Standard ROM tape first sync pulse length in T-states.
pub const SYNC1_PULSE: u32 = 667;

/// Standard ROM tape second sync pulse length in T-states.
pub const SYNC2_PULSE: u32 = 735;

/// Standard ROM tape zero-bit pulse length in T-states.
pub const ZERO_PULSE: u32 = 855;

/// Standard ROM tape one-bit pulse length in T-states.
pub const ONE_PULSE: u32 = 1_710;

/// Standard ROM pilot pulse count for header blocks.
pub const PILOT_COUNT_HEADER: u32 = 8_063;

/// Standard ROM pilot pulse count for data blocks.
pub const PILOT_COUNT_DATA: u32 = 3_223;

/// Standard ROM inter-block pause length in milliseconds.
pub const PAUSE_MS: u32 = 1_000;

/// CPU T-states per millisecond at 3.5 MHz.
pub const TSTATES_PER_MS: u32 = 3_500;

/// One standard-speed tape block ready for playback.
///
/// `data` contains the full byte stream as it appears on tape: flag byte,
/// payload bytes, and checksum.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TapeBlock {
    pub flag: u8,
    pub data: Vec<u8>,
}

/// Tape player that advances through a flat pulse stream.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TapePlayer {
    pulses: Vec<u32>,
    pulse_idx: usize,
    level: bool,
    countdown: u32,
    playing: bool,
}

impl TapePlayer {
    /// Creates an empty tape player with no loaded media.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pulses: Vec::new(),
            pulse_idx: 0,
            level: false,
            countdown: 0,
            playing: false,
        }
    }

    /// Loads a raw pulse stream and rewinds playback to the start.
    pub fn load_pulses(&mut self, pulses: Vec<u32>) {
        self.pulses = pulses;
        self.pulse_idx = 0;
        self.level = false;
        self.countdown = 0;
        self.playing = false;
    }

    /// Converts standard-speed blocks into pulses and loads them.
    pub fn load_blocks(&mut self, blocks: Vec<TapeBlock>) {
        self.load_pulses(standard_blocks_to_pulses(&blocks));
    }

    /// Starts or resumes playback from the current tape position.
    pub fn play(&mut self) {
        if self.playing || self.pulse_idx >= self.pulses.len() {
            return;
        }

        self.playing = true;
        if self.countdown == 0 {
            self.start_current_pulse();
        }
    }

    /// Stops playback without rewinding the current tape position.
    pub fn stop(&mut self) {
        self.playing = false;
    }

    /// Returns whether any tape media is currently loaded.
    #[must_use]
    pub fn has_tape(&self) -> bool {
        !self.pulses.is_empty()
    }

    /// Returns whether playback is currently active.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Returns the current EAR level.
    #[must_use]
    pub fn ear_level(&self) -> bool {
        self.level
    }

    /// Advances the tape by the supplied number of CPU T-states.
    pub fn advance_tstates(&mut self, tstates: u32) {
        if !self.playing {
            return;
        }

        let mut remaining = tstates;
        while remaining > 0 {
            if self.countdown > remaining {
                self.countdown -= remaining;
                return;
            }

            remaining -= self.countdown;
            self.level = !self.level;
            self.pulse_idx += 1;
            self.countdown = 0;

            if self.pulse_idx >= self.pulses.len() {
                self.playing = false;
                return;
            }

            self.start_current_pulse();
        }
    }

    fn start_current_pulse(&mut self) {
        while self.pulse_idx < self.pulses.len() {
            let pulse = self.pulses[self.pulse_idx];
            if pulse == 0 {
                self.level = !self.level;
                self.pulse_idx += 1;
                continue;
            }

            self.countdown = pulse;
            return;
        }

        self.playing = false;
    }
}

impl Default for TapePlayer {
    fn default() -> Self {
        Self::new()
    }
}

fn standard_blocks_to_pulses(blocks: &[TapeBlock]) -> Vec<u32> {
    let mut pulses = Vec::new();
    for block in blocks {
        standard_block_pulses(block, PAUSE_MS, &mut pulses);
    }
    pulses
}

/// Appends the standard ROM pulse sequence for one tape block.
pub fn standard_block_pulses(block: &TapeBlock, pause_ms: u32, pulses: &mut Vec<u32>) {
    let pilot_count = if block.flag < 0x80 {
        PILOT_COUNT_HEADER
    } else {
        PILOT_COUNT_DATA
    };

    for _ in 0..pilot_count {
        pulses.push(PILOT_PULSE);
    }

    pulses.push(SYNC1_PULSE);
    pulses.push(SYNC2_PULSE);
    data_block_pulses(&block.data, 8, ZERO_PULSE, ONE_PULSE, pulses);

    if pause_ms > 0 {
        pulses.push(pause_ms * TSTATES_PER_MS);
    }
}

/// Appends pulses for the supplied data bytes, most-significant bit first.
pub fn data_block_pulses(
    data: &[u8],
    bits_in_last_byte: u8,
    zero_len: u32,
    one_len: u32,
    pulses: &mut Vec<u32>,
) {
    if data.is_empty() {
        return;
    }

    let last_idx = data.len() - 1;
    for (idx, &byte) in data.iter().enumerate() {
        let bits = if idx == last_idx {
            bits_in_last_byte
        } else {
            8
        };
        for bit in (0..bits).rev() {
            let pulse = if byte & (1 << bit) != 0 {
                one_len
            } else {
                zero_len
            };
            pulses.push(pulse);
            pulses.push(pulse);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tape_player_toggles_ear_across_pulses() {
        let mut player = TapePlayer::new();

        assert!(!player.is_playing());
        assert!(!player.ear_level());

        player.load_pulses(vec![100, 200, 300]);
        player.play();
        assert!(player.has_tape());
        assert!(player.is_playing());
        assert!(!player.ear_level());

        player.advance_tstates(100);
        assert!(player.ear_level());

        player.advance_tstates(200);
        assert!(!player.ear_level());

        player.advance_tstates(300);
        assert!(player.ear_level());
        assert!(!player.is_playing());
    }

    #[test]
    fn tape_player_resume_preserves_partial_pulse_progress() {
        let mut player = TapePlayer::new();

        player.load_pulses(vec![10, 10]);
        player.play();
        player.advance_tstates(3);
        player.stop();
        player.play();
        player.advance_tstates(6);
        assert!(!player.ear_level());

        player.advance_tstates(1);
        assert!(player.ear_level());
    }

    #[test]
    fn standard_block_generates_header_pilot_and_sync() {
        let block = TapeBlock {
            flag: 0x00,
            data: vec![0x00; 19],
        };
        let mut pulses = Vec::new();
        standard_block_pulses(&block, 0, &mut pulses);

        assert_eq!(pulses[0], PILOT_PULSE);
        assert_eq!(pulses[PILOT_COUNT_HEADER as usize], SYNC1_PULSE);
        assert_eq!(pulses[PILOT_COUNT_HEADER as usize + 1], SYNC2_PULSE);
    }

    #[test]
    fn data_pulses_follow_bit_values() {
        let mut pulses = Vec::new();

        data_block_pulses(&[0xA5], 8, ZERO_PULSE, ONE_PULSE, &mut pulses);

        assert_eq!(pulses.len(), 16);
        assert_eq!(pulses[0], ONE_PULSE);
        assert_eq!(pulses[1], ONE_PULSE);
        assert_eq!(pulses[2], ZERO_PULSE);
        assert_eq!(pulses[3], ZERO_PULSE);
    }

    #[test]
    fn data_pulses_honor_partial_last_byte() {
        let mut pulses = Vec::new();

        data_block_pulses(&[0xFF], 3, ZERO_PULSE, ONE_PULSE, &mut pulses);

        assert_eq!(pulses.len(), 6);
    }
}
