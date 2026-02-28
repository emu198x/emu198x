//! C64 tape deck state machine.
//!
//! Manages the currently loaded TAP file and tracks which block to
//! deliver next when the ROM tape loading routine is trapped.
//!
//! Also supports real-time pulse playback for turbo loaders that
//! bypass the kernal ROM and read tape signals directly via the
//! CIA1 FLAG pin.

use crate::tap::{C64TapBlock, C64TapFile};

/// Virtual C64 tape deck: holds a TAP file and a block cursor.
pub struct C64TapeDeck {
    tap: Option<C64TapFile>,
    block_index: usize,

    // --- Real-time playback state ---

    /// Raw pulse durations for real-time playback.
    raw_pulses: Vec<u32>,
    /// Current position in the raw pulse stream.
    pulse_index: usize,
    /// Countdown in CPU cycles until the next edge.
    pulse_countdown: u32,
    /// Whether the tape is playing (real-time mode).
    playing: bool,
    /// Datasette motor state (controlled by $01 bit 5).
    motor_on: bool,
    /// Current signal level (toggles on each pulse edge).
    signal_level: bool,
}

impl C64TapeDeck {
    /// Create an empty tape deck (no tape inserted).
    #[must_use]
    pub fn new() -> Self {
        Self {
            tap: None,
            block_index: 0,
            raw_pulses: Vec::new(),
            pulse_index: 0,
            pulse_countdown: 0,
            playing: false,
            motor_on: false,
            signal_level: true,
        }
    }

    /// Insert a TAP file into the deck.
    pub fn insert(&mut self, tap: C64TapFile) {
        self.raw_pulses = tap.raw_pulses.clone();
        self.tap = Some(tap);
        self.block_index = 0;
        self.pulse_index = 0;
        self.pulse_countdown = 0;
        self.playing = false;
    }

    /// Eject the current tape.
    pub fn eject(&mut self) {
        self.tap = None;
        self.block_index = 0;
        self.raw_pulses.clear();
        self.pulse_index = 0;
        self.pulse_countdown = 0;
        self.playing = false;
    }

    /// Whether a tape is loaded.
    #[must_use]
    pub fn is_loaded(&self) -> bool {
        self.tap.is_some()
    }

    /// Return the next block and advance the cursor, or `None` if no more
    /// blocks are available.
    pub fn next_block(&mut self) -> Option<&C64TapBlock> {
        let tap = self.tap.as_ref()?;
        if self.block_index >= tap.blocks.len() {
            return None;
        }
        let block = &tap.blocks[self.block_index];
        self.block_index += 1;
        Some(block)
    }

    /// Rewind the tape to the start.
    pub fn rewind(&mut self) {
        self.block_index = 0;
        self.pulse_index = 0;
        self.pulse_countdown = 0;
        self.signal_level = true;
    }

    /// Current block index (0-based).
    #[must_use]
    pub fn block_index(&self) -> usize {
        self.block_index
    }

    /// Total number of blocks on the tape.
    #[must_use]
    pub fn block_count(&self) -> usize {
        self.tap.as_ref().map_or(0, |t| t.blocks.len())
    }

    // --- Real-time playback ---

    /// Start real-time playback (press PLAY on the datasette).
    pub fn start_play(&mut self) {
        self.playing = true;
    }

    /// Stop playback.
    pub fn stop(&mut self) {
        self.playing = false;
    }

    /// Whether the tape is currently playing.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Set the motor state (from $01 bit 5, active-low).
    pub fn set_motor(&mut self, on: bool) {
        self.motor_on = on;
    }

    /// Whether the motor is running.
    #[must_use]
    pub fn motor_on(&self) -> bool {
        self.motor_on
    }

    /// Tick one CPU cycle during real-time playback.
    ///
    /// Returns `true` when a pulse edge occurs (signal transition).
    /// The caller should feed this to CIA1 `set_flag()`.
    pub fn tick(&mut self) -> bool {
        if !self.playing || !self.motor_on {
            return false;
        }

        if self.pulse_countdown == 0 {
            // Need a new pulse — bail if no more pulses remain
            if self.pulse_index >= self.raw_pulses.len() {
                return false;
            }
            self.pulse_countdown = self.raw_pulses[self.pulse_index];
            self.pulse_index += 1;
        }

        self.pulse_countdown = self.pulse_countdown.saturating_sub(1);

        if self.pulse_countdown == 0 {
            // Edge: toggle signal level
            self.signal_level = !self.signal_level;
            return true;
        }

        false
    }

    /// Whether raw pulses are available for real-time playback.
    #[must_use]
    pub fn has_raw_pulses(&self) -> bool {
        !self.raw_pulses.is_empty()
    }
}

impl Default for C64TapeDeck {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tap::{C64TapBlock, C64TapFile};

    /// Build a TAP file with the given blocks.
    fn make_tap(blocks: Vec<C64TapBlock>) -> C64TapFile {
        C64TapFile {
            blocks,
            raw_pulses: Vec::new(),
        }
    }

    fn sample_block(file_type: u8, start: u16, data: &[u8]) -> C64TapBlock {
        C64TapBlock {
            file_type,
            start_address: start,
            end_address: start + data.len() as u16,
            filename: "TEST".to_string(),
            data: data.to_vec(),
        }
    }

    #[test]
    fn empty_deck() {
        let deck = C64TapeDeck::new();
        assert!(!deck.is_loaded());
        assert_eq!(deck.block_count(), 0);
    }

    #[test]
    fn insert_and_read_blocks() {
        let tap = make_tap(vec![
            sample_block(1, 0x0801, &[1, 2, 3]),
            sample_block(3, 0xC000, &[4, 5]),
        ]);
        let mut deck = C64TapeDeck::new();
        deck.insert(tap);

        assert!(deck.is_loaded());
        assert_eq!(deck.block_count(), 2);
        assert_eq!(deck.block_index(), 0);

        let b1 = deck.next_block().expect("block 1");
        assert_eq!(b1.file_type, 1);
        assert_eq!(b1.data, &[1, 2, 3]);

        let b2 = deck.next_block().expect("block 2");
        assert_eq!(b2.file_type, 3);
        assert_eq!(b2.data, &[4, 5]);

        assert!(deck.next_block().is_none());
    }

    #[test]
    fn rewind() {
        let tap = make_tap(vec![
            sample_block(1, 0x0801, &[1]),
            sample_block(3, 0xC000, &[2]),
        ]);
        let mut deck = C64TapeDeck::new();
        deck.insert(tap);

        let _ = deck.next_block();
        let _ = deck.next_block();
        assert!(deck.next_block().is_none());

        deck.rewind();
        assert_eq!(deck.block_index(), 0);
        assert!(deck.next_block().is_some());
    }

    #[test]
    fn eject() {
        let tap = make_tap(vec![sample_block(1, 0x0801, &[1])]);
        let mut deck = C64TapeDeck::new();
        deck.insert(tap);
        assert!(deck.is_loaded());

        deck.eject();
        assert!(!deck.is_loaded());
        assert!(deck.next_block().is_none());
    }

    #[test]
    fn realtime_tick_produces_edges() {
        let mut deck = C64TapeDeck::new();
        let tap = C64TapFile {
            blocks: Vec::new(),
            raw_pulses: vec![10, 20], // Two pulses: 10 and 20 cycles
        };
        deck.insert(tap);
        deck.start_play();
        deck.set_motor(true);

        // Count all edges over 30 cycles (10 + 20)
        let mut edges = 0;
        let mut edge_ticks = Vec::new();
        for tick in 1..=30 {
            if deck.tick() {
                edges += 1;
                edge_ticks.push(tick);
            }
        }
        assert_eq!(
            edges, 2,
            "Should get 2 edges over 30 cycles, got edges at ticks: {edge_ticks:?}"
        );
    }

    #[test]
    fn no_edges_when_motor_off() {
        let mut deck = C64TapeDeck::new();
        let tap = C64TapFile {
            blocks: Vec::new(),
            raw_pulses: vec![5, 5],
        };
        deck.insert(tap);
        deck.start_play();
        deck.set_motor(false); // Motor off

        let mut edges = 0;
        for _ in 0..20 {
            if deck.tick() {
                edges += 1;
            }
        }
        assert_eq!(edges, 0, "No edges when motor is off");
    }
}
