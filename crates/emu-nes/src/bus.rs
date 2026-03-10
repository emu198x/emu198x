//! NES bus: CPU address routing.
//!
//! Implements `emu_core::Bus` for the NES. Routes CPU addresses to
//! internal RAM, PPU registers, APU, controllers, and cartridge.
//!
//! The NES is fully memory-mapped — there is no separate I/O address space.

#![allow(clippy::cast_possible_truncation)]

use emu_core::{Bus, ReadResult};

use ricoh_apu_2a03::Apu;
use ricoh_ppu_2c02::Ppu;

use crate::cartridge::Mapper;
use crate::controller::{Controller, Zapper};

/// The NES bus, implementing `emu_core::Bus`.
pub struct NesBus {
    /// 2K internal RAM ($0000-$07FF, mirrored to $1FFF).
    pub ram: [u8; 2048],
    /// PPU (2C02).
    pub ppu: Ppu,
    /// APU (2A03).
    pub apu: Apu,
    /// Cartridge mapper.
    pub cartridge: Box<dyn Mapper>,
    /// Controller 1 ($4016).
    pub controller1: Controller,
    /// Controller 2 ($4017 reads).
    pub controller2: Controller,
    /// Zapper light gun (optional, replaces controller 2 reads when present).
    pub zapper: Option<Zapper>,
    /// Controllers 3 and 4 (Four-Score adapter).
    pub controller3: Controller,
    pub controller4: Controller,
    /// Four-Score adapter enabled.
    pub four_score: bool,
    /// Four-Score read counter for $4016 (tracks position in extended sequence).
    four_score_idx_1: u8,
    /// Four-Score read counter for $4017.
    four_score_idx_2: u8,
    /// OAM DMA pending page (set when $4014 is written).
    pub oam_dma_page: Option<u8>,
    /// Tracks whether the last CPU bus cycle was a write (for DMC DMA steal count).
    pub last_cycle_was_write: bool,
}

impl NesBus {
    #[must_use]
    pub fn new(cartridge: Box<dyn Mapper>) -> Self {
        Self::new_with_region(cartridge, crate::config::NesRegion::Ntsc)
    }

    /// Create a bus with the given region for PPU/APU timing.
    #[must_use]
    pub fn new_with_region(cartridge: Box<dyn Mapper>, region: crate::config::NesRegion) -> Self {
        Self {
            ram: [0; 2048],
            ppu: Ppu::new_with_pre_render_line(region.pre_render_line()),
            apu: Apu::new_with_region(match region {
                crate::config::NesRegion::Ntsc => ricoh_apu_2a03::ApuRegion::Ntsc,
                crate::config::NesRegion::Pal => ricoh_apu_2a03::ApuRegion::Pal,
            }),
            cartridge,
            controller1: Controller::new(),
            controller2: Controller::new(),
            zapper: None,
            controller3: Controller::new(),
            controller4: Controller::new(),
            four_score: false,
            four_score_idx_1: 0,
            four_score_idx_2: 0,
            oam_dma_page: None,
            last_cycle_was_write: false,
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
        self.last_cycle_was_write = false;
        let addr = addr as u16;
        let data = match addr {
            0x0000..=0x1FFF => self.ram[(addr & 0x07FF) as usize],
            0x2000..=0x3FFF => {
                let mirroring = self.cartridge.mirroring();
                let cart = self.cartridge.as_mut();
                self.ppu
                    .cpu_read(addr & 0x0007, &mut |a| cart.chr_read(a), mirroring)
            }
            0x4016 => {
                if self.four_score {
                    let idx = self.four_score_idx_1;
                    self.four_score_idx_1 = idx.saturating_add(1);
                    match idx {
                        0..=7 => self.controller1.read(),
                        8..=15 => self.controller3.read(),
                        16 => 0x01, // Signature: bit 0 set for $4016
                        _ => 0,
                    }
                } else {
                    self.controller1.read()
                }
            }
            0x4017 => {
                if let Some(ref z) = self.zapper {
                    z.read()
                } else if self.four_score {
                    let idx = self.four_score_idx_2;
                    self.four_score_idx_2 = idx.saturating_add(1);
                    match idx {
                        0..=7 => self.controller2.read(),
                        8..=15 => self.controller4.read(),
                        16 => 0x02, // Signature: bit 1 set for $4017
                        _ => 0,
                    }
                } else {
                    self.controller2.read()
                }
            }
            0x4000..=0x4015 => self.apu.read(addr),
            0x4018..=0x401F => 0xFF, // APU test mode disabled — open bus
            0x4020..=0xFFFF => self.cartridge.cpu_read(addr),
        };
        ReadResult::new(data)
    }

    fn write(&mut self, addr: u32, value: u8) -> u8 {
        self.last_cycle_was_write = true;
        let addr = addr as u16;
        match addr {
            0x0000..=0x1FFF => self.ram[(addr & 0x07FF) as usize] = value,
            0x2000..=0x3FFF => {
                let mirroring = self.cartridge.mirroring();
                let cart = self.cartridge.as_mut();
                self.ppu.cpu_write(
                    addr & 0x0007,
                    value,
                    &mut |a, v| cart.chr_write(a, v),
                    mirroring,
                );
            }
            0x4014 => {
                // OAM DMA: trigger transfer
                self.oam_dma_page = Some(value);
            }
            0x4016 => {
                self.controller1.write(value);
                self.controller2.write(value);
                self.controller3.write(value);
                self.controller4.write(value);
                if value & 1 == 0 {
                    // Falling edge resets Four-Score read counters
                    self.four_score_idx_1 = 0;
                    self.four_score_idx_2 = 0;
                }
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
    use crate::cartridge::Nrom;
    use ricoh_ppu_2c02::Mirroring;

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

    #[test]
    fn apu_test_mode_reads_return_open_bus() {
        let mut bus = make_bus();
        // $4018-$401F are APU test mode registers — disabled, should return $FF
        for addr in 0x4018..=0x401F {
            assert_eq!(
                bus.read(addr).data,
                0xFF,
                "test mode reg ${addr:04X} should return $FF"
            );
        }
    }

    #[test]
    fn apu_test_mode_writes_are_ignored() {
        let mut bus = make_bus();
        // Writes to test mode registers should be silently ignored
        bus.write(0x4018, 0x42);
        assert_eq!(bus.read(0x4018).data, 0xFF);
    }
}
