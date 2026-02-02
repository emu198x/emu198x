//! Micro-operation definitions for cycle-accurate Z80 execution.
//!
//! Each Z80 instruction is broken down into a sequence of micro-operations.
//! The CPU steps through these one T-state at a time.

#![allow(clippy::match_same_arms)] // T-state counts are intentionally explicit per op.
#![allow(dead_code)] // t_states() will be used for timing verification.

/// A micro-operation that takes one or more T-states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicroOp {
    /// Fetch opcode at PC, increment PC (4 T-states, M1 cycle).
    /// Result stored in `opcode` field.
    FetchOpcode,

    /// Fetch displacement byte for IX/IY+d (3 T-states).
    /// Result stored in `displacement` field.
    FetchDisplacement,

    /// Read immediate byte at PC, increment PC (3 T-states).
    /// Result stored in `data_lo` field.
    ReadImm8,

    /// Read low byte of immediate word at PC, increment PC (3 T-states).
    /// Result stored in `data_lo` field.
    ReadImm16Lo,

    /// Read high byte of immediate word at PC, increment PC (3 T-states).
    /// Result stored in `data_hi` field.
    ReadImm16Hi,

    /// Read byte from address in `addr` field (3 T-states).
    /// Result stored in `data_lo` field.
    ReadMem,

    /// Read low byte from address in `addr` field (3 T-states).
    /// Result stored in `data_lo`, addr incremented.
    ReadMem16Lo,

    /// Read high byte from address in `addr` field (3 T-states).
    /// Result stored in `data_hi`.
    ReadMem16Hi,

    /// Write `data_lo` to address in `addr` field (3 T-states).
    WriteMem,

    /// Write low byte of word to address in `addr` field (3 T-states).
    /// Writes `data_lo` and increments addr for the following high byte write.
    WriteMem16Lo,

    /// Write high byte of word to address in `addr` field (3 T-states).
    /// Writes `data_hi` to current addr.
    WriteMem16Hi,

    /// Write high byte first (for PUSH) - write `data_hi` to addr (3 T-states).
    /// Decrements SP before write.
    WriteMemHiFirst,

    /// Write low byte second (for PUSH) - write `data_lo` to addr (3 T-states).
    /// Decrements SP before write.
    WriteMemLoSecond,

    /// Read from I/O port in `addr` low byte (4 T-states).
    /// Result stored in `data_lo`.
    IoRead,

    /// Write `data_lo` to I/O port in `addr` low byte (4 T-states).
    IoWrite,

    /// Internal operation - just burns T-states (variable count stored in `t_total`).
    Internal,

    /// Execute the decoded instruction.
    /// This performs the actual operation and may queue more micro-ops.
    Execute,
}

/// Duration of each micro-op in T-states.
impl MicroOp {
    #[must_use]
    pub const fn t_states(self) -> u8 {
        match self {
            Self::FetchOpcode => 4,
            Self::FetchDisplacement => 3,
            Self::ReadImm8 => 3,
            Self::ReadImm16Lo => 3,
            Self::ReadImm16Hi => 3,
            Self::ReadMem => 3,
            Self::ReadMem16Lo => 3,
            Self::ReadMem16Hi => 3,
            Self::WriteMem => 3,
            Self::WriteMem16Lo => 3,
            Self::WriteMem16Hi => 3,
            Self::WriteMemHiFirst => 3,
            Self::WriteMemLoSecond => 3,
            Self::IoRead => 4,
            Self::IoWrite => 4,
            Self::Internal => 0, // Variable, handled separately
            Self::Execute => 0,  // Instant
        }
    }
}

/// Queue of pending micro-operations.
/// Fixed size to avoid allocation.
#[derive(Debug, Clone)]
pub struct MicroOpQueue {
    ops: [MicroOp; 16],
    len: u8,
    pos: u8,
}

impl Default for MicroOpQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl MicroOpQueue {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            ops: [MicroOp::FetchOpcode; 16],
            len: 0,
            pos: 0,
        }
    }

    /// Clear the queue and start fresh.
    pub fn clear(&mut self) {
        self.len = 0;
        self.pos = 0;
    }

    /// Push a micro-op onto the queue.
    pub fn push(&mut self, op: MicroOp) {
        debug_assert!((self.len as usize) < self.ops.len(), "MicroOp queue overflow");
        self.ops[self.len as usize] = op;
        self.len += 1;
    }

    /// Get the current micro-op, if any.
    #[must_use]
    pub fn current(&self) -> Option<MicroOp> {
        if self.pos < self.len {
            Some(self.ops[self.pos as usize])
        } else {
            None
        }
    }

    /// Advance to the next micro-op.
    pub fn advance(&mut self) {
        if self.pos < self.len {
            self.pos += 1;
        }
    }

    /// Check if queue is empty (all ops consumed).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pos >= self.len
    }

    /// Get queue length (for debugging).
    #[must_use]
    pub fn len(&self) -> u8 {
        self.len
    }

    /// Get current position (for debugging).
    #[must_use]
    pub fn pos(&self) -> u8 {
        self.pos
    }
}
