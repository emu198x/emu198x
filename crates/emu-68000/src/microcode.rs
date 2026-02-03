//! Micro-operation definitions for cycle-accurate 68000 execution.
//!
//! Each 68000 instruction is broken down into a sequence of micro-operations.
//! The CPU steps through these one clock cycle at a time. The 68000 has a
//! minimum 4-cycle memory access, unlike the Z80's 3-cycle access.
//!
//! Bus cycles on the 68000:
//! - Read/Write: 4 cycles minimum (can be extended by DTACK delay)
//! - Two word reads for long access: 8 cycles

#![allow(clippy::match_same_arms)] // Cycle counts are intentionally explicit per op.
#![allow(dead_code)] // cycles() will be used for timing verification.

/// A micro-operation that takes one or more clock cycles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicroOp {
    /// Fetch instruction word at PC, increment PC by 2 (4 cycles).
    /// Result stored in `opcode` field.
    FetchOpcode,

    /// Fetch extension word at PC, increment PC by 2 (4 cycles).
    /// Result stored in `ext_words` array.
    FetchExtWord,

    /// Read byte from address in `addr` field (4 cycles).
    /// Result stored in `data` field (low byte).
    ReadByte,

    /// Read word from address in `addr` field (4 cycles).
    /// Result stored in `data` field (low word).
    ReadWord,

    /// Read high word of long at `addr` (4 cycles).
    /// Result stored in `data` field (high word).
    ReadLongHi,

    /// Read low word of long at `addr+2` (4 cycles).
    /// Result stored in `data` field (combined with high word).
    ReadLongLo,

    /// Write byte in `data` to address in `addr` (4 cycles).
    WriteByte,

    /// Write word in `data` to address in `addr` (4 cycles).
    WriteWord,

    /// Write high word of `data` to address in `addr` (4 cycles).
    WriteLongHi,

    /// Write low word of `data` to address in `addr+2` (4 cycles).
    WriteLongLo,

    /// Calculate effective address for the current addressing mode.
    /// May queue additional micro-ops for complex modes.
    CalcEA,

    /// Execute the decoded instruction.
    /// This performs the actual operation and may queue more micro-ops.
    Execute,

    /// Internal processing cycles (variable count stored in `internal_cycles`).
    /// Used for multiply, divide, and other complex operations.
    Internal,

    /// Push word onto stack (pre-decrement SP, then write).
    PushWord,

    /// Push long onto stack, high word (pre-decrement SP by 4).
    PushLongHi,
    /// Push long onto stack, low word.
    PushLongLo,

    /// Pop word from stack (read, then post-increment SP).
    PopWord,

    /// Pop long from stack, high word (post-increment SP by 4 after both).
    PopLongHi,
    /// Pop long from stack, low word.
    PopLongLo,

    /// Begin exception processing (saves PC and SR, jumps to vector).
    BeginException,

    /// Read exception vector address.
    ReadVector,

    /// MOVEM write: write one register to memory, advance to next.
    ///
    /// Uses: `ext_words[0]` for mask, `data2` for register index, `addr` for memory address.
    /// Size from `self.size` (Word or Long).
    MovemWrite,

    /// MOVEM read: read one value from memory into register, advance to next.
    ///
    /// Uses: `ext_words[0]` for mask, `data2` for register index, `addr` for memory address.
    /// Size from `self.size` (Word or Long).
    MovemRead,

    /// CMPM: Compare memory (Ay)+,(Ax)+.
    ///
    /// Uses: `addr` for source (Ay), `addr2` for dest (Ax), `data` for Ay reg, `data2` for Ax reg.
    /// Size from `self.size`.
    CmpmExecute,
}

impl MicroOp {
    /// Base number of cycles for this micro-op.
    /// Note: Memory operations may take longer with wait states.
    #[must_use]
    pub const fn cycles(self) -> u8 {
        match self {
            Self::FetchOpcode => 4,
            Self::FetchExtWord => 4,
            Self::ReadByte => 4,
            Self::ReadWord => 4,
            Self::ReadLongHi => 4,
            Self::ReadLongLo => 4,
            Self::WriteByte => 4,
            Self::WriteWord => 4,
            Self::WriteLongHi => 4,
            Self::WriteLongLo => 4,
            Self::CalcEA => 0,      // Varies by mode, instant
            Self::Execute => 0,     // Instant
            Self::Internal => 0,    // Variable, handled separately
            Self::PushWord => 4,
            Self::PushLongHi => 4,
            Self::PushLongLo => 4,
            Self::PopWord => 4,
            Self::PopLongHi => 4,
            Self::PopLongLo => 4,
            Self::BeginException => 0, // Sets up exception, instant
            Self::ReadVector => 4,
            Self::MovemWrite => 4,    // Per word transfer (8 for long = 2 x 4)
            Self::MovemRead => 4,     // Per word transfer
            Self::CmpmExecute => 4,   // Memory read cycle (called twice for two operands)
        }
    }
}

/// Queue of pending micro-operations.
/// Fixed size to avoid allocation.
#[derive(Debug, Clone)]
pub struct MicroOpQueue {
    ops: [MicroOp; 32], // Larger than Z80 due to longer 68000 instructions
    len: u8,
    pos: u8,
}

impl Default for MicroOpQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl MicroOpQueue {
    /// Create a new empty queue.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            ops: [MicroOp::FetchOpcode; 32],
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
        debug_assert!(
            (self.len as usize) < self.ops.len(),
            "MicroOp queue overflow"
        );
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
