//! The programmer-visible registers.

use serde::{Deserialize, Serialize};

/// Carry.
pub const FLAG_C: u8 = 0x01;
/// Zero.
pub const FLAG_Z: u8 = 0x02;
/// IRQ disable.
pub const FLAG_I: u8 = 0x04;
/// Decimal mode.
pub const FLAG_D: u8 = 0x08;
/// Index registers 8-bit. In emulation mode this bit is the break flag and
/// always reads 1.
pub const FLAG_X: u8 = 0x10;
/// Accumulator and memory 8-bit. Always 1 in emulation mode.
pub const FLAG_M: u8 = 0x20;
/// Overflow.
pub const FLAG_V: u8 = 0x40;
/// Negative.
pub const FLAG_N: u8 = 0x80;

/// The 65C816's registers.
///
/// `a`, `x` and `y` are always held at 16 bits. While the M or X flag selects
/// 8-bit width, only the low byte takes part; the high byte of the
/// accumulator survives (it is the B accumulator `XBA` swaps in), while the
/// high bytes of X and Y are forced to zero, as on the chip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registers {
    /// The accumulator, C. The low byte is A and the high byte B.
    pub a: u16,
    pub x: u16,
    pub y: u16,
    /// Stack pointer. In emulation mode the high byte is held at `$01`.
    pub s: u16,
    /// Direct page register.
    pub d: u16,
    /// Data bank register.
    pub dbr: u8,
    /// Program bank register.
    pub pbr: u8,
    pub pc: u16,
    /// Status: `N V M X D I Z C` from bit 7 down.
    pub p: u8,
    /// Emulation mode. Set at reset; changed only by `XCE`.
    pub e: bool,
}

impl Registers {
    /// The state after reset: emulation mode, 8-bit everywhere, IRQs
    /// disabled, decimal mode off, the direct page and both banks at zero and
    /// the stack in page 1.
    #[must_use]
    pub fn new() -> Self {
        Self {
            a: 0,
            x: 0,
            y: 0,
            s: 0x01FF,
            d: 0,
            dbr: 0,
            pbr: 0,
            pc: 0,
            p: FLAG_M | FLAG_X | FLAG_I,
            e: true,
        }
    }

    #[must_use]
    pub fn flag(&self, mask: u8) -> bool {
        self.p & mask != 0
    }

    pub fn set_flag(&mut self, mask: u8, value: bool) {
        if value {
            self.p |= mask;
        } else {
            self.p &= !mask;
        }
    }

    /// Whether the accumulator and memory are 8-bit.
    #[must_use]
    pub fn m8(&self) -> bool {
        self.flag(FLAG_M)
    }

    /// Whether the index registers are 8-bit.
    #[must_use]
    pub fn x8(&self) -> bool {
        self.flag(FLAG_X)
    }

    /// Enforce what emulation mode and an 8-bit index width imply: M and X
    /// set, the stack in page 1, and the index registers' high bytes zero.
    pub fn normalise(&mut self) {
        if self.e {
            self.p |= FLAG_M | FLAG_X;
            self.s = 0x0100 | (self.s & 0xFF);
        }
        if self.x8() {
            self.x &= 0xFF;
            self.y &= 0xFF;
        }
    }
}

impl Default for Registers {
    fn default() -> Self {
        Self::new()
    }
}
