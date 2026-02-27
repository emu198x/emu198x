//! C64 tape deck state machine.
//!
//! Manages the currently loaded TAP file and tracks which block to
//! deliver next when the ROM tape loading routine is trapped.

use crate::tap::{C64TapBlock, C64TapFile};

/// Virtual C64 tape deck: holds a TAP file and a block cursor.
pub struct C64TapeDeck {
    tap: Option<C64TapFile>,
    block_index: usize,
}

impl C64TapeDeck {
    /// Create an empty tape deck (no tape inserted).
    #[must_use]
    pub fn new() -> Self {
        Self {
            tap: None,
            block_index: 0,
        }
    }

    /// Insert a TAP file into the deck.
    pub fn insert(&mut self, tap: C64TapFile) {
        self.tap = Some(tap);
        self.block_index = 0;
    }

    /// Eject the current tape.
    pub fn eject(&mut self) {
        self.tap = None;
        self.block_index = 0;
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
        C64TapFile { blocks }
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
}
