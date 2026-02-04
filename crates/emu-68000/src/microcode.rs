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

    /// TAS: Test and Set byte.
    ///
    /// Uses: `addr` for memory address.
    /// Phase 0: Read byte, set flags. Phase 1: Write byte with bit 7 set.
    TasExecute,

    /// Memory shift/rotate: read word, shift by 1, write back.
    ///
    /// Uses: `addr` for memory address, `data` for shift kind, `data2` for direction.
    /// Kind: 0=AS, 1=LS, 2=ROX, 3=RO. Direction: 0=right, 1=left.
    /// Phase 0: Read word. Phase 1: Shift and write.
    ShiftMemExecute,

    /// ALU operation with memory destination (read-modify-write).
    ///
    /// Uses: `addr` for memory address, `data` for source value (from register),
    /// `data2` for operation:
    ///   0=ADD, 1=SUB, 2=AND, 3=OR, 4=EOR (binary ops with register source)
    ///   5=NEG, 6=NOT, 7=NEGX (unary ops, `data` ignored)
    ///   8=NBCD (BCD negate: 0 - mem - X)
    /// Size from `self.size`.
    /// Phase 0: Read memory. Phase 1: Perform op and write.
    AluMemRmw,

    /// ALU operation with memory source (read from memory, operate, store to register).
    ///
    /// Uses: `addr` for memory address, `data` for destination register number,
    /// `data2` for operation:
    ///   0=ADD, 1=SUB, 2=AND, 3=OR, 4=CMP, 5=ADDA, 6=SUBA, 7=CMPA
    ///   8=TST (just set flags, no register destination)
    ///   9=CHK (check bounds, trigger exception if out of range)
    ///   10=MULU, 11=MULS, 12=DIVU, 13=DIVS (multiply/divide with memory source)
    ///   14=CMPI (compare immediate: memory - `data`, where `data` holds immediate)
    /// Size from `self.size`.
    /// Single phase: Read memory, perform op, store to register.
    AluMemSrc,

    /// Bit operation on memory byte (BTST/BCHG/BCLR/BSET).
    ///
    /// Uses: `addr` for memory address, `data` for bit number (0-7),
    /// `data2` for operation (0=BTST, 1=BCHG, 2=BCLR, 3=BSET).
    /// BTST is read-only, others are read-modify-write.
    /// Phase 0: Read byte. Phase 1 (for BCHG/BCLR/BSET): Write modified byte.
    BitMemOp,

    /// Multi-precision/BCD memory-to-memory: -(Ax),-(Ay).
    ///
    /// Uses: `addr` for source (Ax already pre-decremented),
    /// `addr2` for destination (Ay already pre-decremented),
    /// `data` for source register number (Ax), `data2` for operation:
    ///   0=ABCD (BCD add), 1=SBCD (BCD subtract),
    ///   2=ADDX (binary add), 3=SUBX (binary subtract).
    /// Size from `self.size` (ABCD/SBCD always byte, ADDX/SUBX can vary).
    /// Phase 0: Read src. Phase 1: Read dst. Phase 2: Compute and write result.
    ExtendMemOp,

    /// Copy `data2` to `data` (0 cycles, instant).
    ///
    /// Used during exception processing to load saved SR into `data`
    /// after pushing PC, so PushWord can write the SR.
    SetDataFromData2,
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
            Self::TasExecute => 4,    // Per memory access phase
            Self::ShiftMemExecute => 4, // Per memory access phase
            Self::AluMemRmw => 4,     // Per memory access phase (read then write)
            Self::AluMemSrc => 4,     // Memory read cycle
            Self::BitMemOp => 4,      // Per memory access phase
            Self::ExtendMemOp => 4,   // Per memory access phase (read src, read dst, write)
            Self::SetDataFromData2 => 0, // Instant internal operation
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
