//! NES bus: CPU address routing.
//!
//! Implements `emu_core::Bus` for the NES. Routes CPU addresses to
//! internal RAM, PPU registers, APU, controllers, and cartridge.
//!
//! The NES is fully memory-mapped — there is no separate I/O address space.

#![allow(clippy::cast_possible_truncation)]

use emu_core::{Bus, ReadResult};

use crate::apu::Apu;
use crate::cartridge::Mapper;
use crate::controller::Controller;
use crate::ppu::Ppu;

/// The NES bus, implementing `emu_core::Bus`.
pub struct NesBus {
    /// 2K internal RAM ($0000-$07FF, mirrored to $1FFF).
    pub ram: [u8; 2048],
    /// PPU (2C02).
    pub ppu: Ppu,
    /// APU (stub).
    pub apu: Apu,
    /// Cartridge mapper.
    pub cartridge: Box<dyn Mapper>,
    /// Controller 1 ($4016).
    pub controller1: Controller,
    /// Controller 2 ($4017 reads).
    pub controller2: Controller,
    /// OAM DMA pending page (set when $4014 is written).
    pub oam_dma_page: Option<u8>,
}

impl NesBus {
    #[must_use]
    pub fn new(cartridge: Box<dyn Mapper>) -> Self {
        Self {
            ram: [0; 2048],
            ppu: Ppu::new(),
            apu: Apu::new(),
            cartridge,
            controller1: Controller::new(),
            controller2: Controller::new(),
            oam_dma_page: None,
        }
    }

    /// Peek a byte from RAM without side effects (for observation).
    #[must_use]
    pub fn peek_ram(&self, addr: u16) -> u8 {
        self.ram[(addr & 0x07FF) as usize]
    }
}

impl Bus for NesBus {
    fn read(&mut self, addr: u32) -> ReadResult {
        let addr = addr as u16;
        let data = match addr {
            0x0000..=0x1FFF => self.ram[(addr & 0x07FF) as usize],
            0x2000..=0x3FFF => self.ppu.cpu_read(addr & 0x0007, self.cartridge.as_ref()),
            0x4016 => self.controller1.read(),
            0x4017 => self.controller2.read(),
            0x4000..=0x4015 => self.apu.read(addr),
            0x4018..=0x401F => 0, // Normally disabled APU test mode
            0x4020..=0xFFFF => self.cartridge.cpu_read(addr),
        };
        ReadResult::new(data)
    }

    fn write(&mut self, addr: u32, value: u8) -> u8 {
        let addr = addr as u16;
        match addr {
            0x0000..=0x1FFF => self.ram[(addr & 0x07FF) as usize] = value,
            0x2000..=0x3FFF => {
                self.ppu
                    .cpu_write(addr & 0x0007, value, self.cartridge.as_mut());
            }
            0x4014 => {
                // OAM DMA: trigger transfer
                self.oam_dma_page = Some(value);
            }
            0x4016 => {
                self.controller1.write(value);
                self.controller2.write(value);
            }
            0x4000..=0x4013 | 0x4015 | 0x4017 => self.apu.write(addr, value),
            0x4018..=0x401F => {} // Test mode registers
            0x4020..=0xFFFF => self.cartridge.cpu_write(addr, value),
        }
        0 // No wait states
    }

    // NES doesn't use separate I/O space.
    fn io_read(&mut self, _addr: u32) -> ReadResult {
        ReadResult::new(0xFF)
    }

    fn io_write(&mut self, _addr: u32, _value: u8) -> u8 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::{Mirroring, Nrom};

    fn make_bus() -> NesBus {
        let prg = vec![0xEA; 32768]; // NOPs
        let chr = vec![0; 8192];
        let mapper = Box::new(Nrom::new(prg, chr, Mirroring::Horizontal));
        NesBus::new(mapper)
    }

    #[test]
    fn ram_read_write() {
        let mut bus = make_bus();
        bus.write(0x0000, 0xAB);
        assert_eq!(bus.read(0x0000).data, 0xAB);
        // Mirror at $0800
        assert_eq!(bus.read(0x0800).data, 0xAB);
        // Mirror at $1000
        assert_eq!(bus.read(0x1000).data, 0xAB);
        // Mirror at $1800
        assert_eq!(bus.read(0x1800).data, 0xAB);
    }

    #[test]
    fn cartridge_prg_read() {
        let bus = make_bus();
        // PRG ROM filled with NOPs (0xEA)
        assert_eq!(bus.cartridge.cpu_read(0x8000), 0xEA);
        assert_eq!(bus.cartridge.cpu_read(0xFFFC), 0xEA);
    }

    #[test]
    fn oam_dma_trigger() {
        let mut bus = make_bus();
        assert!(bus.oam_dma_page.is_none());
        bus.write(0x4014, 0x02);
        assert_eq!(bus.oam_dma_page, Some(0x02));
    }
}
