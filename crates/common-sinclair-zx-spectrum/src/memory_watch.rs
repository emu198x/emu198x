//! Z80 memory-write tracer.
//!
//! Captures every CPU write whose target address falls inside a
//! configured `[lo, hi)` range, along with the program counter at
//! the instruction that issued the write. Used by the
//! `watch_memory_*` MCP/script tools to answer "what code touched
//! this byte?" — Amiga's analogous tool with the same shape (see
//! `commodore-agnus-ocs::watch_memory`).
//!
//! Capture is opt-in: the field on the machine core defaults to
//! `None`, the per-cycle cost is one compare-and-branch when no
//! watch is active. When set, the capture cap defaults to
//! [`DEFAULT_WATCH_CAP`] tuples; once full, subsequent matching
//! writes are silently dropped (the cap is reported so the caller
//! can detect saturation).

use serde::{Deserialize, Serialize};

/// Default cap on captured writes. Sized to comfortably cover a
/// frame's worth of writes through a small region (e.g. a screen-
/// attribute strip) without unbounded growth.
pub const DEFAULT_WATCH_CAP: usize = 8192;

/// One captured CPU memory write.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryWriteRecord {
    /// CPU program counter at the time of the write — typically
    /// the address of the instruction that wrote (or one past it,
    /// depending on the instruction's M-cycle layout).
    pub pc: u16,
    /// Target address.
    pub addr: u16,
    /// Byte written.
    pub value: u8,
}

/// CPU memory-write tracer state owned by a Spectrum-class core.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemoryWriteWatch {
    /// Low bound of the watched range (inclusive).
    lo: u16,
    /// High bound of the watched range (exclusive).
    hi: u16,
    /// Captured writes, in chronological order, capped at `cap`.
    writes: Vec<MemoryWriteRecord>,
    /// Maximum number of writes to record before dropping further
    /// matches. Caller can read this back via [`Self::cap`] /
    /// [`Self::is_full`].
    cap: usize,
}

impl MemoryWriteWatch {
    /// Build a watch over `[addr, addr + len)` with the default
    /// capture cap. `len == 0` produces a watch that never matches —
    /// useful as a no-op placeholder when a caller wants to clear
    /// without removing the watch entirely.
    #[must_use]
    pub fn new(addr: u16, len: u16) -> Self {
        Self::with_cap(addr, len, DEFAULT_WATCH_CAP)
    }

    /// Build a watch with an explicit capture cap.
    #[must_use]
    pub fn with_cap(addr: u16, len: u16, cap: usize) -> Self {
        Self {
            lo: addr,
            hi: addr.saturating_add(len),
            writes: Vec::new(),
            cap,
        }
    }

    /// Low bound of the watched range (inclusive).
    #[must_use]
    pub const fn lo(&self) -> u16 {
        self.lo
    }

    /// High bound of the watched range (exclusive).
    #[must_use]
    pub const fn hi(&self) -> u16 {
        self.hi
    }

    /// Maximum number of writes the watch will record before
    /// dropping further matches.
    #[must_use]
    pub const fn cap(&self) -> usize {
        self.cap
    }

    /// Whether the capture buffer has hit `cap` and is dropping
    /// subsequent matches.
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.writes.len() >= self.cap
    }

    /// All captured writes, chronologically.
    #[must_use]
    pub fn records(&self) -> &[MemoryWriteRecord] {
        &self.writes
    }

    /// Drop captured writes (range stays configured).
    pub fn clear(&mut self) {
        self.writes.clear();
    }

    /// Record one CPU write if its address falls inside the watched
    /// range and the buffer is not yet full. Returns `true` when a
    /// record was appended (used by tests; production code ignores
    /// the bool).
    pub fn maybe_record(&mut self, pc: u16, addr: u16, value: u8) -> bool {
        if self.hi <= self.lo {
            return false;
        }
        if addr < self.lo || addr >= self.hi {
            return false;
        }
        if self.writes.len() >= self.cap {
            return false;
        }
        self.writes.push(MemoryWriteRecord { pc, addr, value });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_writes_in_range_and_skips_outside() {
        let mut w = MemoryWriteWatch::new(0x4000, 0x100);
        assert!(w.maybe_record(0x1234, 0x4000, 0xAA));
        assert!(w.maybe_record(0x1234, 0x40FF, 0xBB));
        assert!(!w.maybe_record(0x1234, 0x3FFF, 0xCC));
        assert!(!w.maybe_record(0x1234, 0x4100, 0xDD));
        assert_eq!(w.records().len(), 2);
        assert_eq!(w.records()[0].value, 0xAA);
        assert_eq!(w.records()[1].addr, 0x40FF);
    }

    #[test]
    fn capture_stops_at_cap() {
        let mut w = MemoryWriteWatch::with_cap(0x4000, 0x10, 2);
        assert!(w.maybe_record(0, 0x4000, 1));
        assert!(w.maybe_record(0, 0x4001, 2));
        assert!(!w.maybe_record(0, 0x4002, 3));
        assert!(w.is_full());
        w.clear();
        assert!(!w.is_full());
        assert!(w.maybe_record(0, 0x4000, 9));
    }

    #[test]
    fn zero_length_range_never_records() {
        let mut w = MemoryWriteWatch::new(0x4000, 0);
        assert!(!w.maybe_record(0, 0x4000, 1));
        assert!(w.records().is_empty());
    }
}
