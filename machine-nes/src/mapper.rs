//! NES mapper implementations.
//!
//! Mappers handle cartridge memory banking. Each mapper number corresponds
//! to a specific banking scheme used by different cartridge boards.

use crate::cartridge::Mirroring;

/// Mapper trait for cartridge banking logic.
pub trait Mapper: Send {
    /// Map PRG ROM read address to physical ROM offset.
    fn map_prg_read(&self, addr: u16) -> usize;

    /// Map CHR ROM/RAM read address to physical offset.
    fn map_chr_read(&self, addr: u16) -> usize;

    /// Handle writes to mapper registers ($8000-$FFFF).
    fn write(&mut self, addr: u16, value: u8);

    /// Get current mirroring mode (if mapper controls it).
    fn mirroring(&self) -> Option<Mirroring> {
        None
    }

    /// Handle scanline for IRQ (MMC3).
    fn scanline(&mut self) {}

    /// Check and clear IRQ.
    fn irq(&mut self) -> bool {
        false
    }
}

/// Create a mapper by number.
pub fn create(
    mapper_num: u8,
    prg_size: usize,
    chr_size: usize,
    mirroring: Mirroring,
) -> Result<Box<dyn Mapper>, String> {
    match mapper_num {
        0 => Ok(Box::new(Nrom::new(prg_size))),
        1 => Ok(Box::new(Mmc1::new(prg_size, chr_size))),
        2 => Ok(Box::new(Uxrom::new(prg_size))),
        3 => Ok(Box::new(Cnrom::new(prg_size))),
        4 => Ok(Box::new(Mmc3::new(prg_size, chr_size, mirroring))),
        7 => Ok(Box::new(Axrom::new(prg_size))),
        _ => Err(format!("Unsupported mapper: {}", mapper_num)),
    }
}

/// NROM (Mapper 0) - No banking.
/// Used by: Donkey Kong, Super Mario Bros, etc.
pub struct Nrom {
    prg_size: usize,
}

impl Nrom {
    pub fn new(prg_size: usize) -> Self {
        Self { prg_size }
    }
}

impl Mapper for Nrom {
    fn map_prg_read(&self, addr: u16) -> usize {
        let addr = (addr - 0x8000) as usize;
        if self.prg_size <= 16384 {
            // 16KB ROM: mirror $8000-$BFFF to $C000-$FFFF
            addr & 0x3FFF
        } else {
            addr
        }
    }

    fn map_chr_read(&self, addr: u16) -> usize {
        addr as usize
    }

    fn write(&mut self, _addr: u16, _value: u8) {
        // NROM has no mapper registers
    }
}

/// MMC1 (Mapper 1) - Nintendo's first mapper.
/// Used by: Zelda, Metroid, Final Fantasy, etc.
pub struct Mmc1 {
    prg_size: usize,
    chr_size: usize,
    shift_register: u8,
    write_count: u8,
    control: u8,
    chr_bank0: u8,
    chr_bank1: u8,
    prg_bank: u8,
}

impl Mmc1 {
    pub fn new(prg_size: usize, chr_size: usize) -> Self {
        Self {
            prg_size,
            chr_size,
            shift_register: 0,
            write_count: 0,
            control: 0x0C, // Default: 16KB PRG, fixed last bank
            chr_bank0: 0,
            chr_bank1: 0,
            prg_bank: 0,
        }
    }
}

impl Mapper for Mmc1 {
    fn map_prg_read(&self, addr: u16) -> usize {
        let prg_mode = (self.control >> 2) & 0x03;
        let bank = self.prg_bank as usize & 0x0F;
        let last_bank = (self.prg_size / 16384) - 1;

        match prg_mode {
            0 | 1 => {
                // 32KB mode
                let bank32 = (bank >> 1) * 32768;
                bank32 + ((addr - 0x8000) as usize)
            }
            2 => {
                // Fixed first, switch last
                if addr < 0xC000 {
                    (addr - 0x8000) as usize
                } else {
                    bank * 16384 + ((addr - 0xC000) as usize)
                }
            }
            3 => {
                // Switch first, fixed last
                if addr < 0xC000 {
                    bank * 16384 + ((addr - 0x8000) as usize)
                } else {
                    last_bank * 16384 + ((addr - 0xC000) as usize)
                }
            }
            _ => 0,
        }
    }

    fn map_chr_read(&self, addr: u16) -> usize {
        let chr_mode = (self.control >> 4) & 0x01;

        if self.chr_size == 0 {
            return addr as usize;
        }

        if chr_mode == 0 {
            // 8KB mode
            let bank = (self.chr_bank0 as usize & 0x1E) * 4096;
            bank + (addr as usize)
        } else {
            // 4KB mode
            if addr < 0x1000 {
                (self.chr_bank0 as usize) * 4096 + (addr as usize)
            } else {
                (self.chr_bank1 as usize) * 4096 + ((addr - 0x1000) as usize)
            }
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        if value & 0x80 != 0 {
            // Reset
            self.shift_register = 0;
            self.write_count = 0;
            self.control |= 0x0C;
            return;
        }

        self.shift_register |= (value & 0x01) << self.write_count;
        self.write_count += 1;

        if self.write_count == 5 {
            let data = self.shift_register;

            match addr {
                0x8000..=0x9FFF => self.control = data,
                0xA000..=0xBFFF => self.chr_bank0 = data,
                0xC000..=0xDFFF => self.chr_bank1 = data,
                0xE000..=0xFFFF => self.prg_bank = data,
                _ => {}
            }

            self.shift_register = 0;
            self.write_count = 0;
        }
    }

    fn mirroring(&self) -> Option<Mirroring> {
        Some(match self.control & 0x03 {
            0 => Mirroring::SingleLower,
            1 => Mirroring::SingleUpper,
            2 => Mirroring::Vertical,
            3 => Mirroring::Horizontal,
            _ => unreachable!(),
        })
    }
}

/// UxROM (Mapper 2) - Simple PRG banking.
/// Used by: Mega Man, Castlevania, Contra, etc.
pub struct Uxrom {
    prg_size: usize,
    prg_bank: u8,
}

impl Uxrom {
    pub fn new(prg_size: usize) -> Self {
        Self { prg_size, prg_bank: 0 }
    }
}

impl Mapper for Uxrom {
    fn map_prg_read(&self, addr: u16) -> usize {
        let last_bank = (self.prg_size / 16384) - 1;

        if addr < 0xC000 {
            // Switchable bank
            (self.prg_bank as usize) * 16384 + ((addr - 0x8000) as usize)
        } else {
            // Fixed last bank
            last_bank * 16384 + ((addr - 0xC000) as usize)
        }
    }

    fn map_chr_read(&self, addr: u16) -> usize {
        addr as usize
    }

    fn write(&mut self, _addr: u16, value: u8) {
        self.prg_bank = value & 0x0F;
    }
}

/// CNROM (Mapper 3) - Simple CHR banking.
/// Used by: Arkanoid, Gradius, etc.
pub struct Cnrom {
    prg_size: usize,
    chr_bank: u8,
}

impl Cnrom {
    pub fn new(prg_size: usize) -> Self {
        Self { prg_size, chr_bank: 0 }
    }
}

impl Mapper for Cnrom {
    fn map_prg_read(&self, addr: u16) -> usize {
        let addr = (addr - 0x8000) as usize;
        if self.prg_size <= 16384 {
            addr & 0x3FFF
        } else {
            addr
        }
    }

    fn map_chr_read(&self, addr: u16) -> usize {
        (self.chr_bank as usize) * 8192 + (addr as usize)
    }

    fn write(&mut self, _addr: u16, value: u8) {
        self.chr_bank = value & 0x03;
    }
}

/// MMC3 (Mapper 4) - Scanline counter.
/// Used by: Super Mario Bros 3, Kirby's Adventure, etc.
pub struct Mmc3 {
    prg_size: usize,
    chr_size: usize,
    mirroring: Mirroring,
    bank_select: u8,
    banks: [u8; 8],
    prg_mode: bool,
    chr_mode: bool,
    irq_latch: u8,
    irq_counter: u8,
    irq_reload: bool,
    irq_enabled: bool,
    irq_pending: bool,
}

impl Mmc3 {
    pub fn new(prg_size: usize, chr_size: usize, mirroring: Mirroring) -> Self {
        Self {
            prg_size,
            chr_size,
            mirroring,
            bank_select: 0,
            banks: [0; 8],
            prg_mode: false,
            chr_mode: false,
            irq_latch: 0,
            irq_counter: 0,
            irq_reload: false,
            irq_enabled: false,
            irq_pending: false,
        }
    }
}

impl Mapper for Mmc3 {
    fn map_prg_read(&self, addr: u16) -> usize {
        let last_bank = (self.prg_size / 8192) - 1;
        let second_last = last_bank - 1;

        let bank = match addr {
            0x8000..=0x9FFF => {
                if self.prg_mode {
                    second_last
                } else {
                    self.banks[6] as usize
                }
            }
            0xA000..=0xBFFF => self.banks[7] as usize,
            0xC000..=0xDFFF => {
                if self.prg_mode {
                    self.banks[6] as usize
                } else {
                    second_last
                }
            }
            0xE000..=0xFFFF => last_bank,
            _ => 0,
        };

        (bank % (self.prg_size / 8192)) * 8192 + ((addr & 0x1FFF) as usize)
    }

    fn map_chr_read(&self, addr: u16) -> usize {
        if self.chr_size == 0 {
            return addr as usize;
        }

        let (bank, offset) = if self.chr_mode {
            match addr {
                0x0000..=0x03FF => (self.banks[2], addr),
                0x0400..=0x07FF => (self.banks[3], addr - 0x0400),
                0x0800..=0x0BFF => (self.banks[4], addr - 0x0800),
                0x0C00..=0x0FFF => (self.banks[5], addr - 0x0C00),
                0x1000..=0x17FF => (self.banks[0] & 0xFE, addr - 0x1000),
                0x1800..=0x1FFF => (self.banks[1] & 0xFE, addr - 0x1800),
                _ => (0, 0),
            }
        } else {
            match addr {
                0x0000..=0x07FF => (self.banks[0] & 0xFE, addr),
                0x0800..=0x0FFF => (self.banks[1] & 0xFE, addr - 0x0800),
                0x1000..=0x13FF => (self.banks[2], addr - 0x1000),
                0x1400..=0x17FF => (self.banks[3], addr - 0x1400),
                0x1800..=0x1BFF => (self.banks[4], addr - 0x1800),
                0x1C00..=0x1FFF => (self.banks[5], addr - 0x1C00),
                _ => (0, 0),
            }
        };

        let bank_size = if addr < 0x1000 && !self.chr_mode || addr >= 0x1000 && self.chr_mode {
            1024 // 1KB banks (R0-R1 are 2KB)
        } else {
            1024
        };

        ((bank as usize) * bank_size + (offset as usize)) % self.chr_size
    }

    fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x8000..=0x9FFE if addr & 1 == 0 => {
                self.bank_select = value & 0x07;
                self.prg_mode = value & 0x40 != 0;
                self.chr_mode = value & 0x80 != 0;
            }
            0x8001..=0x9FFF if addr & 1 == 1 => {
                self.banks[self.bank_select as usize] = value;
            }
            0xA000..=0xBFFE if addr & 1 == 0 => {
                self.mirroring = if value & 1 == 0 {
                    Mirroring::Vertical
                } else {
                    Mirroring::Horizontal
                };
            }
            0xC000..=0xDFFE if addr & 1 == 0 => {
                self.irq_latch = value;
            }
            0xC001..=0xDFFF if addr & 1 == 1 => {
                self.irq_reload = true;
            }
            0xE000..=0xFFFE if addr & 1 == 0 => {
                self.irq_enabled = false;
                self.irq_pending = false;
            }
            0xE001..=0xFFFF if addr & 1 == 1 => {
                self.irq_enabled = true;
            }
            _ => {}
        }
    }

    fn mirroring(&self) -> Option<Mirroring> {
        Some(self.mirroring)
    }

    fn scanline(&mut self) {
        if self.irq_counter == 0 || self.irq_reload {
            self.irq_counter = self.irq_latch;
            self.irq_reload = false;
        } else {
            self.irq_counter -= 1;
        }

        if self.irq_counter == 0 && self.irq_enabled {
            self.irq_pending = true;
        }
    }

    fn irq(&mut self) -> bool {
        let pending = self.irq_pending;
        self.irq_pending = false;
        pending
    }
}

/// AxROM (Mapper 7) - Single-screen mirroring.
/// Used by: Battletoads, Wizards & Warriors, etc.
pub struct Axrom {
    prg_size: usize,
    prg_bank: u8,
    mirroring: Mirroring,
}

impl Axrom {
    pub fn new(prg_size: usize) -> Self {
        Self {
            prg_size,
            prg_bank: 0,
            mirroring: Mirroring::SingleLower,
        }
    }
}

impl Mapper for Axrom {
    fn map_prg_read(&self, addr: u16) -> usize {
        let bank = (self.prg_bank & 0x07) as usize;
        (bank * 32768 + ((addr - 0x8000) as usize)) % self.prg_size
    }

    fn map_chr_read(&self, addr: u16) -> usize {
        addr as usize
    }

    fn write(&mut self, _addr: u16, value: u8) {
        self.prg_bank = value & 0x07;
        self.mirroring = if value & 0x10 != 0 {
            Mirroring::SingleUpper
        } else {
            Mirroring::SingleLower
        };
    }

    fn mirroring(&self) -> Option<Mirroring> {
        Some(self.mirroring)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nrom_16k() {
        let mapper = Nrom::new(16384);
        assert_eq!(mapper.map_prg_read(0x8000), 0);
        assert_eq!(mapper.map_prg_read(0xC000), 0); // Mirrored
    }

    #[test]
    fn test_nrom_32k() {
        let mapper = Nrom::new(32768);
        assert_eq!(mapper.map_prg_read(0x8000), 0);
        assert_eq!(mapper.map_prg_read(0xC000), 0x4000);
    }

    #[test]
    fn test_uxrom() {
        let mut mapper = Uxrom::new(128 * 1024); // 128KB
        mapper.write(0x8000, 2);
        assert_eq!(mapper.map_prg_read(0x8000), 2 * 16384);
        // Last bank fixed
        assert_eq!(mapper.map_prg_read(0xC000), 7 * 16384);
    }
}
