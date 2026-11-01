//! The 5A22's eight DMA channels and their HDMA mode.
//!
//! Both are driven by the machine's scheduler (see `lib.rs`), which runs a
//! DMA or HDMA transfer as a stack of resumable frames. The register state
//! and the address arithmetic live here.

use serde::{Deserialize, Serialize};

/// HDMA's bytes per line for each transfer mode.
pub const HDMA_LENGTHS: [u8; 8] = [1, 2, 2, 4, 4, 4, 2, 4];

/// One channel's registers, `$43x0-$43xF`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Channel {
    pub dma_enable: bool,
    pub hdma_enable: bool,
    pub transfer_mode: u8,
    pub fixed_transfer: bool,
    pub reverse_transfer: bool,
    pub unused: bool,
    pub indirect: bool,
    /// `false`: A-bus to B-bus. `true`: B to A.
    pub direction: bool,
    pub target_address: u8,
    pub source_address: u16,
    pub source_bank: u8,
    /// DMA's byte count; HDMA's indirect address shares the register.
    pub transfer_size: u16,
    pub indirect_bank: u8,
    pub hdma_address: u16,
    pub line_counter: u8,
    pub unknown: u8,
    pub hdma_completed: bool,
    pub hdma_do_transfer: bool,
}

impl Default for Channel {
    /// The power-on state ares gives a channel.
    fn default() -> Self {
        Self {
            dma_enable: false,
            hdma_enable: false,
            transfer_mode: 7,
            fixed_transfer: true,
            reverse_transfer: true,
            unused: true,
            indirect: true,
            direction: true,
            target_address: 0xFF,
            source_address: 0xFFFF,
            source_bank: 0xFF,
            transfer_size: 0xFFFF,
            indirect_bank: 0xFF,
            hdma_address: 0xFFFF,
            line_counter: 0xFF,
            unknown: 0xFF,
            hdma_completed: false,
            hdma_do_transfer: false,
        }
    }
}

impl Channel {
    /// HDMA is enabled and has not reached its table's end this frame.
    #[must_use]
    pub fn hdma_active(&self) -> bool {
        self.hdma_enable && !self.hdma_completed
    }

    /// The B-bus address for byte `index` of a transfer unit.
    #[must_use]
    pub fn b_address(&self, index: u8) -> u8 {
        let index = index & 3;
        self.target_address.wrapping_add(match self.transfer_mode {
            1 | 5 => index & 1,
            3 | 7 => index >> 1 & 1,
            4 => index,
            _ => 0,
        })
    }

    /// Read a register, `reg` being the low nibble of `$43xN`.
    #[must_use]
    pub fn read(&self, reg: u8, open_bus: u8) -> u8 {
        match reg {
            0x0 => {
                self.transfer_mode
                    | u8::from(self.fixed_transfer) << 3
                    | u8::from(self.reverse_transfer) << 4
                    | u8::from(self.unused) << 5
                    | u8::from(self.indirect) << 6
                    | u8::from(self.direction) << 7
            }
            0x1 => self.target_address,
            0x2 => self.source_address as u8,
            0x3 => (self.source_address >> 8) as u8,
            0x4 => self.source_bank,
            0x5 => self.transfer_size as u8,
            0x6 => (self.transfer_size >> 8) as u8,
            0x7 => self.indirect_bank,
            0x8 => self.hdma_address as u8,
            0x9 => (self.hdma_address >> 8) as u8,
            0xA => self.line_counter,
            0xB | 0xF => self.unknown,
            _ => open_bus,
        }
    }

    /// Write a register.
    pub fn write(&mut self, reg: u8, data: u8) {
        let lo = |word: &mut u16| *word = *word & 0xFF00 | u16::from(data);
        let hi = |word: &mut u16| *word = *word & 0x00FF | u16::from(data) << 8;
        match reg {
            0x0 => {
                self.transfer_mode = data & 7;
                self.fixed_transfer = data & 0x08 != 0;
                self.reverse_transfer = data & 0x10 != 0;
                self.unused = data & 0x20 != 0;
                self.indirect = data & 0x40 != 0;
                self.direction = data & 0x80 != 0;
            }
            0x1 => self.target_address = data,
            0x2 => lo(&mut self.source_address),
            0x3 => hi(&mut self.source_address),
            0x4 => self.source_bank = data,
            0x5 => lo(&mut self.transfer_size),
            0x6 => hi(&mut self.transfer_size),
            0x7 => self.indirect_bank = data,
            0x8 => lo(&mut self.hdma_address),
            0x9 => hi(&mut self.hdma_address),
            0xA => self.line_counter = data,
            0xB | 0xF => self.unknown = data,
            _ => {}
        }
    }
}

/// Whether the A-bus side of a DMA may touch `address`: not the B-bus or
/// the 5A22's own registers.
#[must_use]
pub fn valid_a(address: u32) -> bool {
    !(address & 0x40_FF00 == 0x2100
        || address & 0x40_FE00 == 0x4000
        || address & 0x40_FFE0 == 0x4200
        || address & 0x40_FF80 == 0x4300)
}

/// A WRAM-to-WRAM transfer through `$2180` does nothing.
#[must_use]
pub fn valid_b(address_a: u32, address_b: u8) -> bool {
    address_b != 0x80 || (address_a & 0xFE_0000 != 0x7E_0000 && address_a & 0x40_E000 != 0)
}
