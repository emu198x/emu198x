//! Micro-operation queue for the 68000 bus state machine.
//!
//! The 68000 executes instructions as sequences of micro-operations. Each
//! micro-op is either a bus cycle (read/write taking 4+ clocks), an internal
//! delay, or an instant operation (execute, promote prefetch).
//!
//! The queue holds pending micro-ops in FIFO order. The tick engine pops
//! from the front and dispatches: instant ops run immediately within the
//! same tick, bus ops enter the `BusCycle` state, internal delays enter
//! the `Internal` state.

use serde::{Deserialize, Serialize};

/// Maximum number of pending micro-operations retained by one CPU.
pub const MICRO_OP_QUEUE_CAPACITY: usize = 32;

/// A single micro-operation in the CPU pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MicroOp {
    // --- Bus operations (4+ clock cycles each) ---
    /// Fetch the next instruction word into IRC.
    FetchIRC,
    /// Read a byte from the current EA address.
    ReadByte,
    /// Read a word from the current EA address.
    ReadWord,
    /// Read a word without storing data (dummy read for 68000 quirks).
    ReadWordNoData,
    /// Read the high word of a long from the current EA address.
    ReadLongHi,
    /// Read the low word of a long from EA address + 2.
    ReadLongLo,
    /// Write a byte to the current EA address.
    WriteByte,
    /// Write a word to the current EA address.
    WriteWord,
    /// Write the high word of a long to the current EA address.
    WriteLongHi,
    /// Write the low word of a long to EA address + 2.
    WriteLongLo,
    /// Push a word onto the stack (SP -= 2, then write).
    PushWord,
    /// Push the high word of a long onto the stack (SP -= 4, write at SP).
    PushLongHi,
    /// Push the low word of a long onto the stack (write at SP + 2).
    PushLongLo,
    /// Pop a word from the stack (read at SP, then SP += 2).
    PopWord,
    /// Pop the high word of a long from the stack (read at SP).
    PopLongHi,
    /// Pop the low word of a long from the stack (read at SP + 2, SP += 4).
    PopLongLo,
    /// Interrupt acknowledge bus cycle (FC = 7, accepted level on A3-A1).
    InterruptAck,

    // --- Internal delay ---
    /// Internal processing delay in CPU clock cycles (4 clocks per unit).
    Internal(u8),

    // --- Instant operations (execute within the same tick) ---
    /// Assert the RESET line on the bus.
    AssertReset,
    /// Run the instruction decoder/executor (decode new or continue follow-up).
    Execute,
    /// Promote IRC to IR and advance the pipeline.
    PromoteIRC,

    // --- MC68020/MC68030 logical long transfers ---
    //
    // Appended after every pre-existing variant so postcard discriminants for
    // saved MC68000/MC68010 micro-op queues remain stable.
    /// Read one logical long operand through the dynamic-sizing sequencer.
    ReadLong,
    /// Write one logical long operand through the dynamic-sizing sequencer.
    WriteLong,
    /// Push one logical long through the dynamic-sizing sequencer.
    PushLong,
    /// Pop one logical long through the dynamic-sizing sequencer.
    PopLong,
}

impl MicroOp {
    /// Returns true if this op completes instantly (no bus cycle or delay).
    ///
    /// Instant ops are processed in a loop within a single tick, allowing
    /// multiple instant ops to chain without advancing the clock.
    #[must_use]
    pub fn is_instant(self) -> bool {
        matches!(
            self,
            Self::AssertReset | Self::Execute | Self::PromoteIRC | Self::Internal(0)
        )
    }

    /// Returns true if this op requires a bus cycle.
    #[must_use]
    pub fn is_bus(self) -> bool {
        !self.is_instant() && !matches!(self, Self::Internal(_))
    }
}

/// Fixed-capacity circular queue of micro-operations.
///
/// The 68000 never needs more than ~20 pending micro-ops for any single
/// instruction, so 32 slots is generous. Uses a circular buffer to avoid
/// allocation.
#[derive(Clone, Serialize, Deserialize)]
pub struct MicroOpQueue {
    ops: [MicroOp; MICRO_OP_QUEUE_CAPACITY],
    head: u8,
    len: u8,
}

impl MicroOpQueue {
    /// Create an empty queue.
    #[must_use]
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            ops: [MicroOp::Internal(0); MICRO_OP_QUEUE_CAPACITY],
            head: 0,
            len: 0,
        }
    }

    /// Push a micro-op to the back of the queue.
    pub fn push(&mut self, op: MicroOp) {
        let idx = (self.head as usize + self.len as usize) % MICRO_OP_QUEUE_CAPACITY;
        self.ops[idx] = op;
        self.len += 1;
    }

    /// Push a micro-op to the front of the queue (used by `consume_irc`
    /// to insert a FetchIRC before whatever comes next).
    pub fn push_front(&mut self, op: MicroOp) {
        self.head = if self.head == 0 {
            (MICRO_OP_QUEUE_CAPACITY - 1) as u8
        } else {
            self.head - 1
        };
        self.ops[self.head as usize] = op;
        self.len += 1;
    }

    /// Pop the front micro-op, or `None` if the queue is empty.
    pub fn pop(&mut self) -> Option<MicroOp> {
        if self.len == 0 {
            return None;
        }
        let op = self.ops[self.head as usize];
        self.head = ((self.head as usize + 1) % MICRO_OP_QUEUE_CAPACITY) as u8;
        self.len -= 1;
        Some(op)
    }

    /// Peek at the front micro-op without removing it.
    #[must_use]
    pub fn front(&self) -> Option<MicroOp> {
        if self.len == 0 {
            None
        } else {
            Some(self.ops[self.head as usize])
        }
    }

    /// Number of pending micro-operations.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len as usize
    }

    /// Maximum number of pending micro-operations retained by the queue.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        MICRO_OP_QUEUE_CAPACITY
    }

    /// Ordered pending operations, followed by `None` in unused slots.
    ///
    /// The fixed array is a diagnostic copy rather than a view of the circular
    /// storage, so consumers see logical FIFO order regardless of wraparound.
    #[must_use]
    pub fn diagnostic_operations(&self) -> [Option<MicroOp>; MICRO_OP_QUEUE_CAPACITY] {
        let mut pending = [None; MICRO_OP_QUEUE_CAPACITY];
        let mut position = 0;
        while position < self.len as usize {
            let index = (self.head as usize + position) % MICRO_OP_QUEUE_CAPACITY;
            pending[position] = Some(self.ops[index]);
            position += 1;
        }
        pending
    }

    /// Returns true if the queue has no pending ops.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Remove all pending ops.
    pub fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }

    /// Format queue contents for debug logging.
    #[must_use]
    pub fn debug_contents(&self) -> String {
        let mut out = String::from("[");
        for i in 0..self.len as usize {
            let idx = (self.head as usize + i) % MICRO_OP_QUEUE_CAPACITY;
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&format!("{:?}", self.ops[idx]));
        }
        out.push(']');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_operations_preserve_fifo_order_across_storage_wrap() {
        let mut queue = MicroOpQueue::new();
        for cycles in 0..30 {
            queue.push(MicroOp::Internal(cycles));
        }
        for _ in 0..29 {
            assert!(queue.pop().is_some());
        }
        queue.push(MicroOp::ReadWord);
        queue.push(MicroOp::WriteByte);
        queue.push(MicroOp::FetchIRC);

        let pending = queue.diagnostic_operations();

        assert_eq!(
            &pending[..4],
            &[
                Some(MicroOp::Internal(29)),
                Some(MicroOp::ReadWord),
                Some(MicroOp::WriteByte),
                Some(MicroOp::FetchIRC),
            ],
        );
        assert!(pending[4..].iter().all(Option::is_none));
        assert_eq!(queue.len(), 4);
        assert_eq!(queue.front(), Some(MicroOp::Internal(29)));
    }
}
