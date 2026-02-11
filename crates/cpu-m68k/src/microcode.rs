//! Micro-operation definitions for cycle-accurate 68000 execution.
//!
//! Each instruction is broken into a sequence of micro-ops that execute
//! one per tick (or instantly for 0-cycle ops). The queue is fixed-size
//! to avoid allocation.

/// Maximum number of micro-ops that can be queued for a single instruction.
const QUEUE_CAPACITY: usize = 32;

/// A single micro-operation in the 68000 execution pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MicroOp {
    // === Prefetch (4 cycles) ===

    /// Read word at PC -> IRC, PC += 2. Takes 4 cycles.
    FetchIRC,

    // === Data reads (4 cycles each) ===

    /// Read byte from self.addr.
    ReadByte,
    /// Read word from self.addr -> self.data.
    ReadWord,
    /// Read word from self.addr -> self.data (high word of long).
    ReadLongHi,
    /// Read word from self.addr+2 -> self.data (low word of long).
    ReadLongLo,

    // === Data writes (4 cycles each) ===

    /// Write byte from self.data to self.addr.
    WriteByte,
    /// Write word from self.data to self.addr.
    WriteWord,
    /// Write high word of self.data to self.addr.
    WriteLongHi,
    /// Write low word of self.data to self.addr+2.
    WriteLongLo,

    // === Stack operations (4 cycles each) ===

    /// SP -= 2, write word from self.data to SP.
    PushWord,
    /// SP -= 4, write high word of self.data to SP.
    PushLongHi,
    /// Write low word of self.data to SP+2.
    PushLongLo,
    /// Read word from SP -> self.data, SP += 2.
    PopWord,
    /// Read high word from SP -> self.data.
    PopLongHi,
    /// Read low word from SP+2 -> self.data, SP += 4.
    PopLongLo,

    // === Internal processing ===

    /// n cycles of internal processing. 0 = instant (no tick consumed).
    Internal(u8),

    // === Instant operations (0 cycles) ===

    /// Decode IR and execute instruction (or followup stage).
    Execute,
}

impl MicroOp {
    /// Returns true if this op completes instantly (no tick consumed).
    pub(crate) fn is_instant(self) -> bool {
        matches!(self, Self::Execute | Self::Internal(0))
    }

    /// Returns the number of cycles this op takes. 0 for instant ops.
    pub(crate) fn cycles(self) -> u8 {
        match self {
            Self::Execute | Self::Internal(0) => 0,
            Self::Internal(n) => n,
            // All bus operations take 4 cycles
            _ => 4,
        }
    }
}

/// Fixed-size queue of micro-ops, used as a FIFO.
#[derive(Debug, Clone)]
pub(crate) struct MicroOpQueue {
    ops: [MicroOp; QUEUE_CAPACITY],
    head: u8,
    len: u8,
}

impl MicroOpQueue {
    /// Create an empty queue.
    pub(crate) fn new() -> Self {
        Self {
            ops: [MicroOp::Internal(0); QUEUE_CAPACITY],
            head: 0,
            len: 0,
        }
    }

    /// Clear the queue.
    pub(crate) fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }

    /// Push a micro-op onto the back of the queue.
    pub(crate) fn push(&mut self, op: MicroOp) {
        debug_assert!(
            (self.len as usize) < QUEUE_CAPACITY,
            "MicroOp queue overflow"
        );
        let idx = (self.head as usize + self.len as usize) % QUEUE_CAPACITY;
        self.ops[idx] = op;
        self.len += 1;
    }

    /// Push a micro-op at the front of the queue.
    pub(crate) fn push_front(&mut self, op: MicroOp) {
        debug_assert!(
            (self.len as usize) < QUEUE_CAPACITY,
            "MicroOp queue overflow"
        );
        self.head = if self.head == 0 {
            (QUEUE_CAPACITY - 1) as u8
        } else {
            self.head - 1
        };
        self.ops[self.head as usize] = op;
        self.len += 1;
    }

    /// Peek at the front of the queue without removing it.
    pub(crate) fn front(&self) -> Option<MicroOp> {
        if self.len == 0 {
            None
        } else {
            Some(self.ops[self.head as usize])
        }
    }

    /// Remove and return the front of the queue.
    pub(crate) fn pop(&mut self) -> Option<MicroOp> {
        if self.len == 0 {
            None
        } else {
            let op = self.ops[self.head as usize];
            self.head = ((self.head as usize + 1) % QUEUE_CAPACITY) as u8;
            self.len -= 1;
            Some(op)
        }
    }

    /// Check if the queue is empty.
    pub(crate) fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Number of ops in the queue.
    #[allow(dead_code)]
    pub(crate) fn len(&self) -> usize {
        self.len as usize
    }
}
