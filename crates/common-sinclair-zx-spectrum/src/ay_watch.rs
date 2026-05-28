//! AY-3-8912 register-write tracer.
//!
//! Captures every CPU write to the AY data port (`$BFFD` on 128K,
//! also on +2 / +2A / +2B / +3 / Pentagon / Scorpion / TS2068)
//! along with the currently-selected register and the program
//! counter at the issuing `OUT` instruction. Used by the
//! `watch_ay_*` MCP / script tools to show curriculum scripts how
//! a music driver or sound-effect routine programs the AY across
//! a window (frame, scene, song bar) — the same shape as
//! [`crate::memory_watch::MemoryWriteWatch`] but routed through
//! the I/O bus rather than memory.
//!
//! Capture is opt-in: the field on each AY-bearing core defaults
//! to `None`. When `Some(_)`, the per-write cost is one push into
//! the buffer; once the buffer hits its cap, further writes are
//! silently dropped (`is_full()` reports the saturation).

use serde::{Deserialize, Serialize};

/// Default cap on captured AY writes. Sized for a few seconds of
/// music: a typical tracker writes 14 of the 16 registers every
/// 50 Hz frame, so 4096 records covers about 6 s of playback —
/// plenty for curriculum chapter analysis without unbounded growth.
pub const DEFAULT_AY_WATCH_CAP: usize = 4096;

/// One captured AY data write.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AyWriteRecord {
    /// CPU program counter at the time of the `OUT ($BFFD),A`.
    pub pc: u16,
    /// AY register index (0-15) that was selected when the write
    /// happened. R0/R1 set tone-A period, R8/R9/R10 set channel
    /// amplitudes, R13 sets envelope shape, and so on.
    pub register: u8,
    /// Byte written to the selected register.
    pub value: u8,
}

/// AY register-write tracer state owned by a Spectrum-class core
/// that carries an AY-3-8912.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AyWriteWatch {
    /// Captured writes, in chronological order, capped at `cap`.
    writes: Vec<AyWriteRecord>,
    /// Maximum records to retain before dropping further matches.
    cap: usize,
}

impl AyWriteWatch {
    /// Build a watch with the default capacity ([`DEFAULT_AY_WATCH_CAP`]).
    #[must_use]
    pub fn new() -> Self {
        Self::with_cap(DEFAULT_AY_WATCH_CAP)
    }

    /// Build a watch with an explicit capture cap.
    #[must_use]
    pub fn with_cap(cap: usize) -> Self {
        Self {
            writes: Vec::new(),
            cap,
        }
    }

    /// Maximum records this watch will retain.
    #[must_use]
    pub const fn cap(&self) -> usize {
        self.cap
    }

    /// `true` once the buffer has hit `cap` and is dropping new
    /// matches.
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.writes.len() >= self.cap
    }

    /// All captured writes, chronologically.
    #[must_use]
    pub fn records(&self) -> &[AyWriteRecord] {
        &self.writes
    }

    /// Drop captured records (capture stays armed for new writes).
    pub fn clear(&mut self) {
        self.writes.clear();
    }

    /// Record one AY write if the buffer is not yet full. Returns
    /// `true` when a record was appended (used by tests; production
    /// code ignores the bool).
    pub fn record(&mut self, pc: u16, register: u8, value: u8) -> bool {
        if self.writes.len() >= self.cap {
            return false;
        }
        self.writes.push(AyWriteRecord {
            pc,
            register,
            value,
        });
        true
    }
}

impl Default for AyWriteWatch {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_writes_until_cap() {
        let mut w = AyWriteWatch::with_cap(2);
        assert!(w.record(0x8000, 0, 0x12));
        assert!(w.record(0x8003, 1, 0x03));
        assert!(!w.record(0x8006, 8, 0x0F), "third write dropped at cap");
        assert!(w.is_full());
        assert_eq!(w.records().len(), 2);
        assert_eq!(w.records()[0].register, 0);
        assert_eq!(w.records()[1].value, 0x03);
    }

    #[test]
    fn clear_drops_records_but_keeps_capture_armed() {
        let mut w = AyWriteWatch::with_cap(4);
        w.record(0, 0, 0);
        w.record(0, 1, 1);
        w.clear();
        assert!(w.records().is_empty());
        assert!(!w.is_full());
        assert!(w.record(0, 2, 2), "should record again after clear");
    }
}
