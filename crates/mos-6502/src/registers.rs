//! 6502 CPU registers.

use crate::flags::{I, U};
use crate::Status;

/// 6502 CPU register set.
///
/// The 6502 has minimal registers:
/// - A: 8-bit accumulator
/// - X, Y: 8-bit index registers
/// - S: 8-bit stack pointer (stack is at $0100-$01FF)
/// - PC: 16-bit program counter
/// - P: 8-bit processor status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Registers {
    /// Accumulator.
    pub a: u8,
    /// X index register.
    pub x: u8,
    /// Y index register.
    pub y: u8,
    /// Stack pointer (points to next free location, stack at $0100-$01FF).
    pub s: u8,
    /// Program counter.
    pub pc: u16,
    /// Processor status flags.
    pub p: Status,
}

impl Default for Registers {
    fn default() -> Self {
        Self::new()
    }
}

impl Registers {
    /// Create registers in reset state.
    ///
    /// After reset:
    /// - A, X, Y are undefined (we use 0)
    /// - S is decremented by 3 from its previous value (we use $FD)
    /// - PC is loaded from reset vector at $FFFC-$FFFD
    /// - I flag is set, D flag is cleared (on NMOS 6502)
    #[must_use]
    pub const fn new() -> Self {
        Self {
            a: 0,
            x: 0,
            y: 0,
            s: 0xFD,
            pc: 0,
            // On reset, the 6502 sets I and leaves other flags undefined; we
            // model this as only U and I set.
            p: Status(U | I),
        }
    }

    /// Push a value onto the stack, return the address written.
    pub fn push(&mut self) -> u16 {
        let addr = 0x0100 | u16::from(self.s);
        self.s = self.s.wrapping_sub(1);
        addr
    }

    /// Pop a value from the stack, return the address to read.
    pub fn pop(&mut self) -> u16 {
        self.s = self.s.wrapping_add(1);
        0x0100 | u16::from(self.s)
    }

    /// Get the current stack address without modifying S.
    #[must_use]
    pub const fn stack_addr(&self) -> u16 {
        0x0100 | (self.s as u16)
    }
}
