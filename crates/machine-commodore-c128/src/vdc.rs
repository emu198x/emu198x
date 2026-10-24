//! The 8563 VDC's CPU interface: its registers and its 16 KB of video RAM.
//!
//! This is what the KERNAL needs to bring the 80-column chip up: the address
//! register at `$D600`, the data register at `$D601`, the update address
//! (R18/R19) that R31 reads and writes through, and the block fill and copy
//! that a write to R30 starts. It follows VICE's `vdc-mem.c`. The VDC does
//! not draw yet, so nothing reaches a screen; that is the next step.
//!
//! Two simplifications stand until it draws. The VDC is always ready: VICE
//! approximates how long each access keeps it busy, and the KERNAL only waits
//! on the ready bit. And its vertical blanking flag follows a fixed raster
//! rather than the VDC's own timing registers.

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

/// The flat C128's VDC has 16 KB.
const VRAM_SIZE: usize = 0x4000;

/// Bits that read as 1 in each register (VICE `regmask`).
const REGISTER_MASK: [u8; 38] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0xE0, 0x00, 0x00, 0xFC, 0xE0, 0x80, 0xE0, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xE0, 0x00, 0x00, 0x00, 0x00, 0x0F, 0xE0, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0xF0, 0x3F,
];

/// The revision in the status register's low bits: an 8563 R9.
const REVISION: u8 = 2;

/// Phi2 cycles per frame and the share of them in the active display, for the
/// status register's blanking flag (a PAL frame, 200 of 312 lines).
const FRAME_CYCLES: u64 = 312 * 63;
const ACTIVE_CYCLES: u64 = 200 * 63;

/// The 8563 VDC.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Vdc {
    #[serde(with = "BigArray")]
    regs: [u8; 38],
    /// The register `$D601` reaches.
    selected: u8,
    #[serde(with = "BigArray")]
    vram: [u8; VRAM_SIZE],
}

impl Vdc {
    pub(crate) fn new() -> Self {
        Self {
            regs: [0; 38],
            selected: 0,
            vram: [0; VRAM_SIZE],
        }
    }

    /// Read `$D600` (even addresses) or `$D601` (odd). `cycle` is the phi2
    /// count, for the blanking flag.
    pub(crate) fn read(&mut self, addr: u16, cycle: u64) -> u8 {
        if addr & 1 == 0 {
            let blanking = cycle % FRAME_CYCLES >= ACTIVE_CYCLES;
            return 0x80 | REVISION | if blanking { 0x20 } else { 0 };
        }
        match self.selected {
            31 => {
                let value = self.vram_read(self.update_address());
                self.set_update_address(self.update_address().wrapping_add(1));
                value
            }
            reg @ 0..=37 => self.regs[usize::from(reg)] | REGISTER_MASK[usize::from(reg)],
            _ => 0xFF,
        }
    }

    /// Read without side effects.
    pub(crate) fn peek(&self, addr: u16) -> u8 {
        if addr & 1 == 0 {
            return 0x80 | REVISION;
        }
        match self.selected {
            31 => self.vram_read(self.update_address()),
            reg @ 0..=37 => self.regs[usize::from(reg)] | REGISTER_MASK[usize::from(reg)],
            _ => 0xFF,
        }
    }

    /// Write `$D600` (select a register) or `$D601` (write it).
    pub(crate) fn write(&mut self, addr: u16, value: u8) {
        if addr & 1 == 0 {
            self.selected = value & 0x3F;
            return;
        }
        let reg = self.selected;
        if reg >= 38 {
            return;
        }
        match reg {
            // The light pen position is read-only.
            16 | 17 => {}
            30 => {
                self.regs[30] = value;
                self.fill_or_copy();
            }
            31 => {
                self.regs[31] = value;
                let address = self.update_address();
                self.vram_write(address, value);
                self.set_update_address(address.wrapping_add(1));
            }
            _ => self.regs[usize::from(reg)] = value,
        }
    }

    /// A byte of video RAM, for inspection.
    pub(crate) fn vram(&self) -> &[u8] {
        &self.vram
    }

    fn update_address(&self) -> u16 {
        u16::from(self.regs[18]) << 8 | u16::from(self.regs[19])
    }

    fn set_update_address(&mut self, address: u16) {
        self.regs[18] = (address >> 8) as u8;
        self.regs[19] = address as u8;
    }

    fn vram_read(&self, address: u16) -> u8 {
        self.vram[usize::from(address) & (VRAM_SIZE - 1)]
    }

    fn vram_write(&mut self, address: u16, value: u8) {
        self.vram[usize::from(address) & (VRAM_SIZE - 1)] = value;
    }

    /// R30 starts a block operation of R30 bytes (0 means 256) at the update
    /// address: a copy from the block start (R32/R33) when R24 bit 7 is set,
    /// otherwise a fill with R31. Both leave the addresses past the block.
    fn fill_or_copy(&mut self) {
        let length = if self.regs[30] == 0 {
            256
        } else {
            u16::from(self.regs[30])
        };
        let start = self.update_address();
        if self.regs[24] & 0x80 != 0 {
            let source = u16::from(self.regs[32]) << 8 | u16::from(self.regs[33]);
            for i in 0..length {
                let byte = self.vram_read(source.wrapping_add(i));
                self.vram_write(start.wrapping_add(i), byte);
            }
            let end = source.wrapping_add(length);
            self.regs[31] = self.vram_read(end.wrapping_sub(1));
            self.regs[32] = (end >> 8) as u8;
            self.regs[33] = end as u8;
        } else {
            for i in 0..length {
                self.vram_write(start.wrapping_add(i), self.regs[31]);
            }
        }
        self.set_update_address(start.wrapping_add(length));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(vdc: &mut Vdc, reg: u8, value: u8) {
        vdc.write(0xD600, reg);
        vdc.write(0xD601, value);
    }

    #[test]
    fn data_goes_through_the_update_address_and_advances_it() {
        let mut vdc = Vdc::new();
        set(&mut vdc, 18, 0x10);
        set(&mut vdc, 19, 0x00);
        set(&mut vdc, 31, 0x41);
        set(&mut vdc, 31, 0x42);
        assert_eq!(&vdc.vram()[0x1000..0x1002], &[0x41, 0x42]);
        set(&mut vdc, 19, 0x00);
        vdc.write(0xD600, 31);
        assert_eq!(vdc.read(0xD601, 0), 0x41);
        assert_eq!(vdc.read(0xD601, 0), 0x42);
    }

    #[test]
    fn r30_fills_and_copies_blocks() {
        let mut vdc = Vdc::new();
        set(&mut vdc, 18, 0x00);
        set(&mut vdc, 19, 0x00);
        set(&mut vdc, 31, 0x20); // writes one byte, then...
        set(&mut vdc, 30, 0x09); // ...nine more of it
        assert!(vdc.vram()[..10].iter().all(|&b| b == 0x20));
        assert_eq!(vdc.vram()[10], 0);
        set(&mut vdc, 24, 0x80); // copy mode
        set(&mut vdc, 32, 0x00);
        set(&mut vdc, 33, 0x00);
        set(&mut vdc, 18, 0x08);
        set(&mut vdc, 19, 0x00);
        set(&mut vdc, 30, 0x00); // 256 bytes
        assert!(vdc.vram()[0x800..0x80A].iter().all(|&b| b == 0x20));
        vdc.write(0xD600, 18);
        assert_eq!(
            vdc.read(0xD601, 0),
            0x09,
            "the update address is past the block"
        );
    }

    #[test]
    fn the_status_register_is_ready_and_reports_blanking() {
        let mut vdc = Vdc::new();
        assert_eq!(vdc.read(0xD600, 0), 0x82);
        assert_eq!(vdc.read(0xD600, ACTIVE_CYCLES), 0xA2);
        vdc.write(0xD600, 5);
        assert_eq!(vdc.read(0xD601, 0), 0xE0, "unimplemented bits read as 1");
    }
}
