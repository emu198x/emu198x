//! NES memory map and bus implementation.
//!
//! Memory map:
//! - $0000-$07FF: 2KB internal RAM
//! - $0800-$1FFF: Mirrors of RAM
//! - $2000-$2007: PPU registers
//! - $2008-$3FFF: Mirrors of PPU registers
//! - $4000-$4017: APU and I/O registers
//! - $4018-$401F: Normally disabled APU/IO
//! - $4020-$FFFF: Cartridge space (PRG ROM/RAM)

use crate::cartridge::Cartridge;
use emu_core::Bus;

/// NES memory subsystem.
pub struct NesMemory {
    /// 2KB internal RAM.
    ram: [u8; 2048],
    /// 2KB PPU VRAM (nametables).
    vram: [u8; 2048],
    /// 32-byte palette RAM.
    palette: [u8; 32],
    /// Loaded cartridge.
    cartridge: Option<Cartridge>,
    /// Controller 1 shift register.
    controller_shift: u8,
    /// Controller 1 current state.
    pub controller_state: u8,
    /// Controller strobe latch.
    controller_strobe: bool,
    /// Pending PPU writes for external rendering.
    ppu_writes: Vec<(u16, u8)>,
    /// Pending APU writes (address, value).
    pub(crate) apu_writes: Vec<(u16, u8)>,
    /// OAM DMA pending (page address).
    pub(crate) oam_dma_pending: Option<u8>,
    /// Pending PPU register writes (register 0-7, value).
    pub(crate) ppu_reg_writes: Vec<(u8, u8)>,
    /// PPU status register ($2002) - updated by PPU.
    pub(crate) ppu_status: u8,
    /// PPU OAM data for reads ($2004).
    pub(crate) ppu_oam_data: u8,
    /// PPU VRAM read buffer ($2007).
    pub(crate) ppu_data_buffer: u8,
    /// Last value written to any PPU register (open bus).
    ppu_latch: u8,
}

impl NesMemory {
    /// Create new NES memory.
    pub fn new() -> Self {
        Self {
            ram: [0; 2048],
            vram: [0; 2048],
            palette: [0; 32],
            cartridge: None,
            controller_shift: 0,
            controller_state: 0,
            controller_strobe: false,
            ppu_writes: Vec::new(),
            apu_writes: Vec::new(),
            oam_dma_pending: None,
            ppu_reg_writes: Vec::new(),
            ppu_status: 0,
            ppu_oam_data: 0,
            ppu_data_buffer: 0,
            ppu_latch: 0,
        }
    }

    /// Load a cartridge.
    pub fn load_cartridge(&mut self, cartridge: Cartridge) {
        self.cartridge = Some(cartridge);
    }

    /// Read from PPU address space ($0000-$3FFF).
    pub fn ppu_read(&self, addr: u16) -> u8 {
        match addr {
            // Pattern tables (CHR ROM/RAM)
            0x0000..=0x1FFF => {
                if let Some(ref cart) = self.cartridge {
                    cart.chr_read(addr)
                } else {
                    0
                }
            }
            // Nametables
            0x2000..=0x3EFF => {
                let mirrored = self.mirror_nametable(addr);
                self.vram[mirrored as usize]
            }
            // Palette
            0x3F00..=0x3FFF => {
                let index = (addr & 0x1F) as usize;
                // Addresses $3F10/$3F14/$3F18/$3F1C mirror $3F00/$3F04/$3F08/$3F0C
                let index = if index >= 0x10 && (index & 0x03) == 0 {
                    index - 0x10
                } else {
                    index
                };
                self.palette[index]
            }
            _ => 0,
        }
    }

    /// Write to PPU address space ($0000-$3FFF).
    pub fn ppu_write(&mut self, addr: u16, value: u8) {
        self.ppu_writes.push((addr, value));

        match addr {
            // Pattern tables (CHR RAM only)
            0x0000..=0x1FFF => {
                if let Some(ref mut cart) = self.cartridge {
                    cart.chr_write(addr, value);
                }
            }
            // Nametables
            0x2000..=0x3EFF => {
                let mirrored = self.mirror_nametable(addr);
                self.vram[mirrored as usize] = value;
            }
            // Palette
            0x3F00..=0x3FFF => {
                let index = (addr & 0x1F) as usize;
                let index = if index >= 0x10 && (index & 0x03) == 0 {
                    index - 0x10
                } else {
                    index
                };
                self.palette[index] = value;
            }
            _ => {}
        }
    }

    /// Mirror nametable address based on cartridge mirroring mode.
    fn mirror_nametable(&self, addr: u16) -> u16 {
        let addr = addr & 0x2FFF;
        let index = (addr - 0x2000) & 0x0FFF;

        if let Some(ref cart) = self.cartridge {
            cart.mirror_nametable(index)
        } else {
            // Default: vertical mirroring
            index & 0x07FF
        }
    }

    /// Take pending PPU writes.
    pub fn take_ppu_writes(&mut self) -> Vec<(u16, u8)> {
        std::mem::take(&mut self.ppu_writes)
    }

    /// Take pending APU writes.
    pub fn take_apu_writes(&mut self) -> Vec<(u16, u8)> {
        std::mem::take(&mut self.apu_writes)
    }

    /// Take pending PPU register writes.
    pub fn take_ppu_reg_writes(&mut self) -> Vec<(u8, u8)> {
        std::mem::take(&mut self.ppu_reg_writes)
    }

    /// Take OAM DMA request.
    pub fn take_oam_dma(&mut self) -> Option<u8> {
        self.oam_dma_pending.take()
    }

    /// Get palette data.
    pub fn palette(&self) -> &[u8; 32] {
        &self.palette
    }
}

impl Default for NesMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl Bus for NesMemory {
    fn read(&mut self, addr: u32) -> u8 {
        let addr = addr as u16;
        match addr {
            // RAM and mirrors
            0x0000..=0x1FFF => self.ram[(addr & 0x07FF) as usize],
            // PPU registers (mirrored every 8 bytes)
            0x2000..=0x3FFF => {
                let reg = addr & 0x07;
                match reg {
                    // $2002 - Status (clears vblank flag, handled by NES step)
                    2 => {
                        let status = self.ppu_status;
                        self.ppu_status &= 0x7F; // Clear vblank flag
                        // Queue a status read event for PPU to handle toggle reset
                        self.ppu_reg_writes.push((0x82, 0)); // Special: 0x80 | reg = status read
                        status
                    }
                    // $2004 - OAM data
                    4 => self.ppu_oam_data,
                    // $2007 - VRAM data (buffered read)
                    7 => {
                        // Queue a read event for PPU to update buffer
                        self.ppu_reg_writes.push((0x87, 0)); // Special: 0x80 | reg = data read
                        self.ppu_data_buffer
                    }
                    // Write-only registers return latch
                    _ => self.ppu_latch,
                }
            }
            // APU/IO registers
            0x4000..=0x4015 => 0, // TODO: APU reads
            // Controller 1
            0x4016 => {
                if self.controller_strobe {
                    self.controller_state & 1
                } else {
                    let bit = self.controller_shift & 1;
                    self.controller_shift >>= 1;
                    bit
                }
            }
            // Controller 2
            0x4017 => 0, // TODO: Controller 2
            // Cartridge space
            0x4020..=0xFFFF => {
                if let Some(ref cart) = self.cartridge {
                    cart.prg_read(addr)
                } else {
                    0
                }
            }
            _ => 0,
        }
    }

    fn write(&mut self, addr: u32, value: u8) {
        let addr = addr as u16;
        match addr {
            // RAM and mirrors
            0x0000..=0x1FFF => self.ram[(addr & 0x07FF) as usize] = value,
            // PPU registers (mirrored every 8 bytes)
            0x2000..=0x3FFF => {
                let reg = (addr & 0x07) as u8;
                self.ppu_latch = value;
                self.ppu_reg_writes.push((reg, value));
            }
            // APU registers
            0x4000..=0x4013 => {
                self.apu_writes.push((addr, value));
            }
            // OAM DMA
            0x4014 => {
                self.oam_dma_pending = Some(value);
            }
            // APU status
            0x4015 => {
                self.apu_writes.push((addr, value));
            }
            // APU frame counter
            0x4017 => {
                self.apu_writes.push((addr, value));
            }
            // Controller strobe
            0x4016 => {
                self.controller_strobe = value & 1 != 0;
                if self.controller_strobe {
                    self.controller_shift = self.controller_state;
                }
            }
            // Cartridge space
            0x4020..=0xFFFF => {
                if let Some(ref mut cart) = self.cartridge {
                    cart.prg_write(addr, value);
                }
            }
            _ => {}
        }
    }

    fn tick(&mut self, _cycles: u32) {
        // Cycle counting handled by NES main loop
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ram_mirror() {
        let mut mem = NesMemory::new();
        mem.write(0x0000, 0x42);
        assert_eq!(mem.read(0x0000), 0x42);
        assert_eq!(mem.read(0x0800), 0x42); // Mirror 1
        assert_eq!(mem.read(0x1000), 0x42); // Mirror 2
        assert_eq!(mem.read(0x1800), 0x42); // Mirror 3
    }

    #[test]
    fn test_palette_mirror() {
        let mut mem = NesMemory::new();
        mem.ppu_write(0x3F00, 0x0F); // Background color
        assert_eq!(mem.ppu_read(0x3F00), 0x0F);
        assert_eq!(mem.ppu_read(0x3F10), 0x0F); // Mirror
    }
}
