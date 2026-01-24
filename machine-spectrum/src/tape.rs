//! Tape emulation with accurate pulse generation.
//!
//! This module provides cycle-accurate tape playback by generating the actual
//! pulse waveforms that would come from a real tape. This is essential for
//! turbo loaders and copy protection schemes that use non-standard timing.

/// Standard pulse lengths in T-states (from Spectrum ROM).
pub mod timing {
    /// Pilot pulse length (2168 T-states).
    pub const PILOT_PULSE: u32 = 2168;

    /// Number of pilot pulses for header block.
    pub const PILOT_HEADER_PULSES: u32 = 8063;

    /// Number of pilot pulses for data block.
    pub const PILOT_DATA_PULSES: u32 = 3223;

    /// Sync pulse 1 length (667 T-states).
    pub const SYNC1_PULSE: u32 = 667;

    /// Sync pulse 2 length (735 T-states).
    pub const SYNC2_PULSE: u32 = 735;

    /// Zero bit pulse length (855 T-states, two pulses per bit).
    pub const ZERO_PULSE: u32 = 855;

    /// One bit pulse length (1710 T-states, two pulses per bit).
    pub const ONE_PULSE: u32 = 1710;

    /// Pause between blocks in T-states (1 second at 3.5MHz).
    pub const BLOCK_PAUSE: u32 = 3_500_000;
}

/// Represents a single pulse (high or low transition).
#[derive(Debug, Clone, Copy)]
struct Pulse {
    /// Duration of this pulse in T-states.
    duration: u32,
    /// Level after this pulse (true = high, false = low).
    level: bool,
}

/// Tape state for pulse-accurate playback.
pub struct Tape {
    /// Raw TAP data.
    data: Vec<u8>,
    /// Current position in TAP data (byte offset).
    data_pos: usize,
    /// Generated pulse sequence for current block.
    pulses: Vec<Pulse>,
    /// Current position in pulse sequence.
    pulse_index: usize,
    /// T-states elapsed within current pulse.
    pulse_t_state: u32,
    /// Current EAR level output.
    ear_level: bool,
    /// Whether tape is playing.
    playing: bool,
    /// Whether to use instant loading (ROM trap) or pulse generation.
    instant_load: bool,
}

impl Tape {
    /// Create a new empty tape.
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            data_pos: 0,
            pulses: Vec::new(),
            pulse_index: 0,
            pulse_t_state: 0,
            ear_level: false,
            playing: false,
            instant_load: true, // Default to instant loading for compatibility
        }
    }

    /// Load TAP data into the tape.
    pub fn load(&mut self, data: Vec<u8>) {
        self.data = data;
        self.data_pos = 0;
        self.pulses.clear();
        self.pulse_index = 0;
        self.pulse_t_state = 0;
        self.ear_level = false;
        self.playing = false;
    }

    /// Clear the tape.
    pub fn clear(&mut self) {
        self.data.clear();
        self.data_pos = 0;
        self.pulses.clear();
        self.pulse_index = 0;
        self.pulse_t_state = 0;
        self.ear_level = false;
        self.playing = false;
    }

    /// Enable or disable instant loading mode.
    ///
    /// When enabled, the ROM trap is used for loading (faster but less accurate).
    /// When disabled, pulse generation is used (accurate, supports turbo loaders).
    pub fn set_instant_load(&mut self, enabled: bool) {
        self.instant_load = enabled;
    }

    /// Check if instant loading is enabled.
    pub fn instant_load(&self) -> bool {
        self.instant_load
    }

    /// Start tape playback.
    pub fn play(&mut self) {
        if !self.data.is_empty() {
            self.playing = true;
            // Generate pulses for the first block if we haven't already
            if self.pulses.is_empty() && self.data_pos < self.data.len() {
                self.generate_next_block();
            }
        }
    }

    /// Stop tape playback.
    pub fn stop(&mut self) {
        self.playing = false;
    }

    /// Check if tape is playing.
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Get current EAR level.
    pub fn ear_level(&self) -> bool {
        self.ear_level
    }

    /// Advance tape by the given number of T-states.
    ///
    /// Updates the EAR level based on pulse timing.
    pub fn tick(&mut self, t_states: u32) {
        if !self.playing || self.instant_load {
            return;
        }

        let mut remaining = t_states;

        while remaining > 0 && self.pulse_index < self.pulses.len() {
            let pulse = &self.pulses[self.pulse_index];
            let time_left_in_pulse = pulse.duration.saturating_sub(self.pulse_t_state);

            if remaining >= time_left_in_pulse {
                // Move to next pulse
                remaining -= time_left_in_pulse;
                self.ear_level = pulse.level;
                self.pulse_index += 1;
                self.pulse_t_state = 0;
            } else {
                // Stay in current pulse
                self.pulse_t_state += remaining;
                remaining = 0;
            }
        }

        // If we've finished all pulses, try to generate the next block
        if self.pulse_index >= self.pulses.len() {
            if self.data_pos < self.data.len() {
                self.generate_next_block();
            } else {
                // End of tape
                self.playing = false;
            }
        }
    }

    /// Generate pulse sequence for the next TAP block.
    fn generate_next_block(&mut self) {
        self.pulses.clear();
        self.pulse_index = 0;
        self.pulse_t_state = 0;

        // Read block length (2 bytes, little-endian)
        if self.data_pos + 2 > self.data.len() {
            return;
        }

        let block_len =
            self.data[self.data_pos] as usize | ((self.data[self.data_pos + 1] as usize) << 8);
        self.data_pos += 2;

        if block_len == 0 || self.data_pos + block_len > self.data.len() {
            return;
        }

        let block_data = &self.data[self.data_pos..self.data_pos + block_len];
        self.data_pos += block_len;

        // Determine if this is a header or data block
        let is_header = block_len == 19 && !block_data.is_empty() && block_data[0] == 0x00;

        // Generate pilot tone
        let pilot_count = if is_header {
            timing::PILOT_HEADER_PULSES
        } else {
            timing::PILOT_DATA_PULSES
        };

        let mut level = false; // Start low
        for _ in 0..pilot_count {
            level = !level;
            self.pulses.push(Pulse {
                duration: timing::PILOT_PULSE,
                level,
            });
        }

        // Sync pulses
        level = !level;
        self.pulses.push(Pulse {
            duration: timing::SYNC1_PULSE,
            level,
        });
        level = !level;
        self.pulses.push(Pulse {
            duration: timing::SYNC2_PULSE,
            level,
        });

        // Data bytes
        for &byte in block_data {
            for bit in (0..8).rev() {
                let is_one = (byte >> bit) & 1 != 0;
                let pulse_len = if is_one {
                    timing::ONE_PULSE
                } else {
                    timing::ZERO_PULSE
                };

                // Two pulses per bit
                level = !level;
                self.pulses.push(Pulse {
                    duration: pulse_len,
                    level,
                });
                level = !level;
                self.pulses.push(Pulse {
                    duration: pulse_len,
                    level,
                });
            }
        }

        // Pause between blocks (end low)
        self.pulses.push(Pulse {
            duration: timing::BLOCK_PAUSE,
            level: false,
        });
    }

    /// Get the next block for ROM trap loading.
    ///
    /// Returns the block data (including flag and checksum) or None if no more blocks.
    pub fn next_block_for_trap(&mut self) -> Option<Vec<u8>> {
        if self.data_pos + 2 > self.data.len() {
            return None;
        }

        let len =
            self.data[self.data_pos] as usize | ((self.data[self.data_pos + 1] as usize) << 8);

        self.data_pos += 2;

        if self.data_pos + len > self.data.len() {
            return None;
        }

        let block = self.data[self.data_pos..self.data_pos + len].to_vec();
        self.data_pos += len;
        Some(block)
    }
}

impl Default for Tape {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tape_returns_no_block() {
        let mut tape = Tape::new();
        assert!(tape.next_block_for_trap().is_none());
    }

    #[test]
    fn load_clears_state() {
        let mut tape = Tape::new();
        tape.playing = true;
        tape.ear_level = true;
        tape.data_pos = 100;

        tape.load(vec![0x13, 0x00, 0x00]); // 19-byte block header

        assert!(!tape.playing);
        assert!(!tape.ear_level);
        assert_eq!(tape.data_pos, 0);
    }

    #[test]
    fn next_block_advances_position() {
        // Create a minimal TAP with two blocks
        let mut tap_data = vec![];

        // Block 1: 3 bytes (flag + 1 data byte + checksum)
        tap_data.push(0x03);
        tap_data.push(0x00);
        tap_data.push(0xFF); // flag
        tap_data.push(0xAA); // data
        tap_data.push(0x55); // checksum

        // Block 2: 3 bytes
        tap_data.push(0x03);
        tap_data.push(0x00);
        tap_data.push(0x00); // flag
        tap_data.push(0xBB); // data
        tap_data.push(0xBB); // checksum

        let mut tape = Tape::new();
        tape.load(tap_data);

        let block1 = tape.next_block_for_trap().unwrap();
        assert_eq!(block1, vec![0xFF, 0xAA, 0x55]);

        let block2 = tape.next_block_for_trap().unwrap();
        assert_eq!(block2, vec![0x00, 0xBB, 0xBB]);

        assert!(tape.next_block_for_trap().is_none());
    }

    #[test]
    fn pulse_generation_starts_with_pilot() {
        // Create a minimal data block (not a header)
        let mut tap_data = vec![];
        tap_data.push(0x03);
        tap_data.push(0x00);
        tap_data.push(0xFF); // flag (data block)
        tap_data.push(0x00); // data
        tap_data.push(0xFF); // checksum

        let mut tape = Tape::new();
        tape.load(tap_data);
        tape.set_instant_load(false);
        tape.play();

        // Should have generated pilot pulses
        assert!(!tape.pulses.is_empty());

        // Data block should have PILOT_DATA_PULSES pilot pulses
        // followed by sync, data, and pause
        let expected_pilot_pulses = timing::PILOT_DATA_PULSES as usize;
        assert!(tape.pulses.len() > expected_pilot_pulses);

        // First pilot pulse should be the standard length
        assert_eq!(tape.pulses[0].duration, timing::PILOT_PULSE);
    }

    #[test]
    fn tick_advances_through_pulses() {
        let mut tap_data = vec![];
        tap_data.push(0x03);
        tap_data.push(0x00);
        tap_data.push(0xFF);
        tap_data.push(0x00);
        tap_data.push(0xFF);

        let mut tape = Tape::new();
        tape.load(tap_data);
        tape.set_instant_load(false);
        tape.play();

        let initial_index = tape.pulse_index;

        // Tick past one pulse
        tape.tick(timing::PILOT_PULSE + 1);

        // Should have advanced
        assert!(tape.pulse_index > initial_index);
    }

    #[test]
    fn ear_level_toggles_with_pulses() {
        let mut tap_data = vec![];
        tap_data.push(0x03);
        tap_data.push(0x00);
        tap_data.push(0xFF);
        tap_data.push(0x00);
        tap_data.push(0xFF);

        let mut tape = Tape::new();
        tape.load(tap_data);
        tape.set_instant_load(false);
        tape.play();

        let level1 = tape.ear_level();

        // Advance past one pulse
        tape.tick(timing::PILOT_PULSE);
        let level2 = tape.ear_level();

        // Level should have toggled
        assert_ne!(level1, level2);
    }

    #[test]
    fn instant_load_skips_pulse_generation() {
        let mut tap_data = vec![];
        tap_data.push(0x03);
        tap_data.push(0x00);
        tap_data.push(0xFF);
        tap_data.push(0x00);
        tap_data.push(0xFF);

        let mut tape = Tape::new();
        tape.load(tap_data);
        tape.set_instant_load(true);
        tape.play();

        // With instant load, tick should not affect anything
        let ear_before = tape.ear_level();
        tape.tick(1_000_000);
        let ear_after = tape.ear_level();

        assert_eq!(ear_before, ear_after);
    }
}
