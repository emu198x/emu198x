//! iNES cartridge parser and mapper implementations.
//!
//! Parses the iNES file format (header + PRG ROM + CHR ROM) and provides
//! a `Mapper` trait for address translation. Supports NROM (Mapper 0),
//! MMC1 (Mapper 1), UxROM (Mapper 2), CNROM (Mapper 3), MMC3 (Mapper 4),
//! AxROM (Mapper 7), and MMC2 (Mapper 9).

#![allow(clippy::cast_possible_truncation)]

/// Nametable mirroring mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mirroring {
    Horizontal,
    Vertical,
    FourScreen,
    SingleScreenLower,
    SingleScreenUpper,
}

/// Parsed iNES file header.
#[derive(Debug)]
#[allow(dead_code)]
pub struct CartridgeHeader {
    pub prg_rom_banks: u8,
    pub chr_rom_banks: u8,
    pub mapper_number: u8,
    pub mirroring: Mirroring,
    pub has_battery: bool,
}

/// Mapper trait: translates CPU and PPU addresses to cartridge ROM/RAM.
///
/// `chr_read` takes `&mut self` because some mappers (MMC2, MMC4) update
/// internal latches when the PPU reads from pattern table addresses.
pub trait Mapper {
    fn cpu_read(&self, addr: u16) -> u8;
    fn cpu_write(&mut self, addr: u16, value: u8);
    fn chr_read(&mut self, addr: u16) -> u8;
    fn chr_write(&mut self, addr: u16, value: u8);
    fn mirroring(&self) -> Mirroring;
    /// Whether the mapper is asserting an IRQ. Default: no IRQ.
    fn irq_pending(&self) -> bool {
        false
    }
}

/// NROM (Mapper 0): no bank switching.
///
/// - PRG: 16K mirrored at $8000-$FFFF, or 32K at $8000-$FFFF
/// - CHR: 8K at PPU $0000-$1FFF (ROM or RAM if `chr_rom_banks` == 0)
pub struct Nrom {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    mirroring: Mirroring,
}

impl Nrom {
    #[must_use]
    pub fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>, mirroring: Mirroring) -> Self {
        let chr_is_ram = chr_data.is_empty();
        let chr = if chr_is_ram {
            vec![0u8; 8192] // 8K CHR RAM
        } else {
            chr_data
        };
        Self {
            prg_rom,
            chr,
            chr_is_ram,
            mirroring,
        }
    }
}

impl Mapper for Nrom {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => {
                let offset = (addr - 0x8000) as usize;
                if self.prg_rom.len() == 16384 {
                    // 16K: mirror $8000-$BFFF to $C000-$FFFF
                    self.prg_rom[offset % 16384]
                } else {
                    // 32K: direct mapping
                    self.prg_rom[offset % self.prg_rom.len()]
                }
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, _addr: u16, _value: u8) {
        // NROM has no writable PRG area
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        self.chr[(addr as usize) & 0x1FFF]
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        if self.chr_is_ram {
            self.chr[(addr as usize) & 0x1FFF] = value;
        }
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

/// MMC1 (Mapper 1, SxROM): serial shift register bank switching.
///
/// - 5-bit shift register loaded one bit at a time via writes to $8000-$FFFF
/// - After 5 writes, value dispatched to internal register based on address
/// - Writing with bit 7 set resets shift register and sets PRG mode 3
/// - PRG: 16K or 32K banking modes
/// - CHR: 4K or 8K banking modes
/// - PRG RAM: 8K at $6000-$7FFF
struct Mmc1 {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    prg_ram: [u8; 8192],
    shift_register: u8,
    shift_count: u8,
    control: u8,
    chr_bank_0: u8,
    chr_bank_1: u8,
    prg_bank: u8,
}

impl Mmc1 {
    fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>) -> Self {
        let chr_is_ram = chr_data.is_empty();
        let chr = if chr_is_ram {
            vec![0u8; 8192] // 8K CHR RAM
        } else {
            chr_data
        };
        Self {
            prg_rom,
            chr,
            chr_is_ram,
            prg_ram: [0; 8192],
            shift_register: 0,
            shift_count: 0,
            control: 0x0C, // PRG mode 3 (fix last bank) on power-up
            chr_bank_0: 0,
            chr_bank_1: 0,
            prg_bank: 0,
        }
    }

    /// Number of 16K PRG banks.
    fn prg_bank_count(&self) -> usize {
        self.prg_rom.len() / 16384
    }

    /// Read a byte from PRG ROM at a given 16K bank + offset.
    fn read_prg(&self, bank: usize, offset: usize) -> u8 {
        let bank = bank % self.prg_bank_count();
        self.prg_rom[bank * 16384 + offset]
    }

    /// Write to the shift register or reset it.
    fn write_register(&mut self, addr: u16, value: u8) {
        if value & 0x80 != 0 {
            // Reset: clear shift register, set PRG mode 3
            self.shift_register = 0;
            self.shift_count = 0;
            self.control |= 0x0C;
            return;
        }

        // Shift bit 0 of value into the register (LSB first)
        self.shift_register |= (value & 1) << self.shift_count;
        self.shift_count += 1;

        if self.shift_count == 5 {
            let data = self.shift_register;
            // Dispatch based on address bits 14:13
            match (addr >> 13) & 0x03 {
                0 => self.control = data,    // $8000-$9FFF
                1 => self.chr_bank_0 = data, // $A000-$BFFF
                2 => self.chr_bank_1 = data, // $C000-$DFFF
                3 => self.prg_bank = data,   // $E000-$FFFF
                _ => unreachable!(),
            }
            self.shift_register = 0;
            self.shift_count = 0;
        }
    }
}

impl Mapper for Mmc1 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                // PRG RAM (active-low enable in prg_bank bit 4, but many
                // games assume always enabled — default to enabled)
                self.prg_ram[(addr - 0x6000) as usize]
            }
            0x8000..=0xBFFF => {
                let offset = (addr - 0x8000) as usize;
                let prg_mode = (self.control >> 2) & 0x03;
                match prg_mode {
                    0 | 1 => {
                        // 32K mode: bit 0 of bank number ignored
                        let bank = self.prg_bank as usize & 0x0E;
                        self.read_prg(bank, offset)
                    }
                    2 => {
                        // Fix first: $8000 = bank 0
                        self.read_prg(0, offset)
                    }
                    3 => {
                        // Switch: $8000 = selected bank
                        self.read_prg(self.prg_bank as usize & 0x0F, offset)
                    }
                    _ => unreachable!(),
                }
            }
            0xC000..=0xFFFF => {
                let offset = (addr - 0xC000) as usize;
                let prg_mode = (self.control >> 2) & 0x03;
                match prg_mode {
                    0 | 1 => {
                        // 32K mode: second 16K of the 32K block
                        let bank = self.prg_bank as usize & 0x0E;
                        self.read_prg(bank + 1, offset)
                    }
                    2 => {
                        // Fix first: $C000 = selected bank
                        self.read_prg(self.prg_bank as usize & 0x0F, offset)
                    }
                    3 => {
                        // Fix last: $C000 = last bank
                        self.read_prg(self.prg_bank_count() - 1, offset)
                    }
                    _ => unreachable!(),
                }
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                self.prg_ram[(addr - 0x6000) as usize] = value;
            }
            0x8000..=0xFFFF => {
                self.write_register(addr, value);
            }
            _ => {}
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        let addr = addr as usize & 0x1FFF;
        let chr_mode = (self.control >> 4) & 1;

        if chr_mode == 0 {
            // 8K mode: bit 0 of chr_bank_0 ignored
            let bank = (self.chr_bank_0 as usize & 0x1E) * 4096;
            let index = (bank + addr) % self.chr.len();
            self.chr[index]
        } else {
            // 4K mode
            let bank = if addr < 0x1000 {
                self.chr_bank_0 as usize
            } else {
                self.chr_bank_1 as usize
            };
            let offset = addr & 0x0FFF;
            let index = (bank * 4096 + offset) % self.chr.len();
            self.chr[index]
        }
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        if !self.chr_is_ram {
            return;
        }
        let addr = addr as usize & 0x1FFF;
        let chr_mode = (self.control >> 4) & 1;

        if chr_mode == 0 {
            let bank = (self.chr_bank_0 as usize & 0x1E) * 4096;
            let index = (bank + addr) % self.chr.len();
            self.chr[index] = value;
        } else {
            let bank = if addr < 0x1000 {
                self.chr_bank_0 as usize
            } else {
                self.chr_bank_1 as usize
            };
            let offset = addr & 0x0FFF;
            let index = (bank * 4096 + offset) % self.chr.len();
            self.chr[index] = value;
        }
    }

    fn mirroring(&self) -> Mirroring {
        match self.control & 0x03 {
            0 => Mirroring::SingleScreenLower,
            1 => Mirroring::SingleScreenUpper,
            2 => Mirroring::Vertical,
            3 => Mirroring::Horizontal,
            _ => unreachable!(),
        }
    }
}

/// UxROM (Mapper 2): simple 16K PRG bank switching.
///
/// One of the most common NES mappers, used by Mega Man, Castlevania,
/// Contra, and DuckTales.
///
/// - PRG: 16K switchable at $8000-$BFFF, 16K fixed (last bank) at $C000-$FFFF
/// - CHR: 8K RAM (most boards) or ROM
/// - Mirroring: fixed from cartridge header
struct UxRom {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    mirroring: Mirroring,
    prg_bank: u8,
}

impl UxRom {
    fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>, mirroring: Mirroring) -> Self {
        let chr_is_ram = chr_data.is_empty();
        let chr = if chr_is_ram {
            vec![0u8; 8192]
        } else {
            chr_data
        };
        Self {
            prg_rom,
            chr,
            chr_is_ram,
            mirroring,
            prg_bank: 0,
        }
    }

    fn prg_bank_count(&self) -> usize {
        self.prg_rom.len() / 16384
    }
}

impl Mapper for UxRom {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xBFFF => {
                let bank = self.prg_bank as usize % self.prg_bank_count();
                let offset = (addr - 0x8000) as usize;
                self.prg_rom[bank * 16384 + offset]
            }
            0xC000..=0xFFFF => {
                // Fixed to last bank
                let bank = self.prg_bank_count() - 1;
                let offset = (addr - 0xC000) as usize;
                self.prg_rom[bank * 16384 + offset]
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            // Bus conflict: written value ANDed with ROM data at write address
            let rom_byte = self.cpu_read(addr);
            self.prg_bank = value & rom_byte;
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        self.chr[(addr as usize) & 0x1FFF]
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        if self.chr_is_ram {
            self.chr[(addr as usize) & 0x1FFF] = value;
        }
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

/// CNROM (Mapper 3): simple 8K CHR bank switching.
///
/// Used by many early NES games including Gradius, Paperboy, and
/// Arkanoid. PRG ROM is unbanked (16K mirrored or 32K). Writes to
/// $8000-$FFFF select an 8K CHR ROM bank at PPU $0000-$1FFF.
struct CnRom {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: Mirroring,
    chr_bank: u8,
}

impl CnRom {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self {
            prg_rom,
            chr_rom,
            mirroring,
            chr_bank: 0,
        }
    }
}

impl Mapper for CnRom {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => {
                let offset = (addr - 0x8000) as usize;
                self.prg_rom[offset % self.prg_rom.len()]
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            // Bus conflict: written value ANDed with ROM data at write address
            let rom_byte = self.cpu_read(addr);
            self.chr_bank = value & rom_byte;
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        let bank_offset = self.chr_bank as usize * 8192;
        let index = (bank_offset + (addr as usize & 0x1FFF)) % self.chr_rom.len();
        self.chr_rom[index]
    }

    fn chr_write(&mut self, _addr: u16, _value: u8) {
        // CNROM uses CHR ROM — no writes
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

/// AxROM (Mapper 7): 32K PRG bank switching with single-screen mirroring.
///
/// Used by Battletoads, Marble Madness, and Wizards & Warriors.
///
/// - PRG: 32K switchable at $8000-$FFFF
/// - CHR: 8K RAM (always)
/// - Mirroring: single-screen, selected by bit 4 of bank register
struct AxRom {
    prg_rom: Vec<u8>,
    chr_ram: [u8; 8192],
    bank: u8,
    mirroring: Mirroring,
}

impl AxRom {
    fn new(prg_rom: Vec<u8>) -> Self {
        Self {
            prg_rom,
            chr_ram: [0; 8192],
            bank: 0,
            mirroring: Mirroring::SingleScreenLower,
        }
    }

    fn prg_bank_count(&self) -> usize {
        self.prg_rom.len() / 32768
    }
}

impl Mapper for AxRom {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => {
                let bank = (self.bank as usize & 0x07) % self.prg_bank_count();
                let offset = (addr - 0x8000) as usize;
                self.prg_rom[bank * 32768 + offset]
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            // Bus conflict: written value ANDed with ROM data at write address
            let effective = value & self.cpu_read(addr);
            self.bank = effective & 0x07;
            self.mirroring = if effective & 0x10 != 0 {
                Mirroring::SingleScreenUpper
            } else {
                Mirroring::SingleScreenLower
            };
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        self.chr_ram[(addr as usize) & 0x1FFF]
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        self.chr_ram[(addr as usize) & 0x1FFF] = value;
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

/// MMC3 (Mapper 4, TxROM): the second-most common NES mapper.
///
/// Used by SMB3, Kirby's Adventure, Mega Man 3-6, and Batman.
///
/// - PRG: 4 x 8K windows with two switchable modes
/// - CHR: 8 x 1K windows (mixed 2K/1K granularity) with two modes
/// - PRG RAM: 8K at $6000-$7FFF with write protection
/// - Mirroring: dynamically switchable H/V
/// - Scanline counter: IRQ driven by PPU A12 rising edges
struct Mmc3 {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    prg_ram: [u8; 8192],
    /// Bank select register ($8000): bits 0-2 = target register,
    /// bit 6 = PRG mode, bit 7 = CHR mode.
    bank_select: u8,
    /// R0-R7 bank registers, written via $8001.
    registers: [u8; 8],
    mirroring: Mirroring,
    prg_ram_enable: bool,
    prg_ram_write_protect: bool,
    irq_latch: u8,
    irq_counter: u8,
    irq_reload_flag: bool,
    irq_enabled: bool,
    irq_pending: bool,
    /// Last observed state of PPU A12 (for rising edge detection).
    last_a12: bool,
}

impl Mmc3 {
    fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>) -> Self {
        let chr_is_ram = chr_data.is_empty();
        let chr = if chr_is_ram {
            vec![0u8; 8192]
        } else {
            chr_data
        };
        Self {
            prg_rom,
            chr,
            chr_is_ram,
            prg_ram: [0; 8192],
            bank_select: 0,
            registers: [0; 8],
            mirroring: Mirroring::Vertical,
            prg_ram_enable: true,
            prg_ram_write_protect: false,
            irq_latch: 0,
            irq_counter: 0,
            irq_reload_flag: false,
            irq_enabled: false,
            irq_pending: false,
            last_a12: false,
        }
    }

    /// Number of 8K PRG banks.
    fn prg_8k_count(&self) -> usize {
        self.prg_rom.len() / 8192
    }

    /// Read a byte from an 8K PRG bank at the given offset.
    fn read_prg_8k(&self, bank: usize, offset: usize) -> u8 {
        let bank = bank % self.prg_8k_count();
        self.prg_rom[bank * 8192 + offset]
    }

    /// Clock the scanline counter on PPU A12 rising edge.
    fn clock_irq_counter(&mut self) {
        if self.irq_counter == 0 || self.irq_reload_flag {
            self.irq_counter = self.irq_latch;
            self.irq_reload_flag = false;
        } else {
            self.irq_counter -= 1;
        }
        if self.irq_counter == 0 && self.irq_enabled {
            self.irq_pending = true;
        }
    }
}

impl Mapper for Mmc3 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                if self.prg_ram_enable {
                    self.prg_ram[(addr - 0x6000) as usize]
                } else {
                    0
                }
            }
            0x8000..=0x9FFF => {
                let offset = (addr - 0x8000) as usize;
                if self.bank_select & 0x40 == 0 {
                    // Mode 0: R6 at $8000
                    self.read_prg_8k(self.registers[6] as usize & 0x3F, offset)
                } else {
                    // Mode 1: second-to-last at $8000
                    self.read_prg_8k(self.prg_8k_count() - 2, offset)
                }
            }
            0xA000..=0xBFFF => {
                let offset = (addr - 0xA000) as usize;
                // R7 at $A000 in both modes
                self.read_prg_8k(self.registers[7] as usize & 0x3F, offset)
            }
            0xC000..=0xDFFF => {
                let offset = (addr - 0xC000) as usize;
                if self.bank_select & 0x40 == 0 {
                    // Mode 0: second-to-last at $C000
                    self.read_prg_8k(self.prg_8k_count() - 2, offset)
                } else {
                    // Mode 1: R6 at $C000
                    self.read_prg_8k(self.registers[6] as usize & 0x3F, offset)
                }
            }
            0xE000..=0xFFFF => {
                let offset = (addr - 0xE000) as usize;
                // Last bank always at $E000
                self.read_prg_8k(self.prg_8k_count() - 1, offset)
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                if self.prg_ram_enable && !self.prg_ram_write_protect {
                    self.prg_ram[(addr - 0x6000) as usize] = value;
                }
            }
            0x8000..=0x9FFF => {
                if addr & 1 == 0 {
                    // $8000 (even): bank select
                    self.bank_select = value;
                } else {
                    // $8001 (odd): bank data
                    let reg = (self.bank_select & 0x07) as usize;
                    self.registers[reg] = value;
                }
            }
            0xA000..=0xBFFF => {
                if addr & 1 == 0 {
                    // $A000 (even): mirroring
                    self.mirroring = if value & 1 == 0 {
                        Mirroring::Vertical
                    } else {
                        Mirroring::Horizontal
                    };
                } else {
                    // $A001 (odd): PRG RAM protect
                    self.prg_ram_write_protect = value & 0x40 != 0;
                    self.prg_ram_enable = value & 0x80 != 0;
                }
            }
            0xC000..=0xDFFF => {
                if addr & 1 == 0 {
                    // $C000 (even): IRQ latch
                    self.irq_latch = value;
                } else {
                    // $C001 (odd): IRQ reload
                    self.irq_reload_flag = true;
                }
            }
            0xE000..=0xFFFF => {
                if addr & 1 == 0 {
                    // $E000 (even): IRQ disable + acknowledge
                    self.irq_enabled = false;
                    self.irq_pending = false;
                } else {
                    // $E001 (odd): IRQ enable
                    self.irq_enabled = true;
                }
            }
            _ => {}
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        // Track A12 for scanline counter
        let a12 = addr & 0x1000 != 0;
        if a12 && !self.last_a12 {
            self.clock_irq_counter();
        }
        self.last_a12 = a12;

        let addr_usize = (addr & 0x1FFF) as usize;
        let chr_mode = self.bank_select & 0x80 != 0;

        // Resolve 1K bank index for this address
        let bank_1k = if !chr_mode {
            // Mode 0: 2K,2K,1K,1K,1K,1K
            match addr_usize >> 10 {
                0 => (self.registers[0] & 0xFE) as usize,     // R0 (2K-aligned)
                1 => (self.registers[0] | 1) as usize,        // R0+1
                2 => (self.registers[1] & 0xFE) as usize,     // R1 (2K-aligned)
                3 => (self.registers[1] | 1) as usize,        // R1+1
                4 => self.registers[2] as usize,               // R2
                5 => self.registers[3] as usize,               // R3
                6 => self.registers[4] as usize,               // R4
                7 => self.registers[5] as usize,               // R5
                _ => unreachable!(),
            }
        } else {
            // Mode 1: 1K,1K,1K,1K,2K,2K (inverted)
            match addr_usize >> 10 {
                0 => self.registers[2] as usize,               // R2
                1 => self.registers[3] as usize,               // R3
                2 => self.registers[4] as usize,               // R4
                3 => self.registers[5] as usize,               // R5
                4 => (self.registers[0] & 0xFE) as usize,     // R0 (2K-aligned)
                5 => (self.registers[0] | 1) as usize,        // R0+1
                6 => (self.registers[1] & 0xFE) as usize,     // R1 (2K-aligned)
                7 => (self.registers[1] | 1) as usize,        // R1+1
                _ => unreachable!(),
            }
        };

        let offset = addr_usize & 0x3FF; // 1K offset within bank
        let index = (bank_1k * 1024 + offset) % self.chr.len();
        self.chr[index]
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        if !self.chr_is_ram {
            return;
        }

        // Track A12 for scanline counter
        let a12 = addr & 0x1000 != 0;
        if a12 && !self.last_a12 {
            self.clock_irq_counter();
        }
        self.last_a12 = a12;

        let addr_usize = (addr & 0x1FFF) as usize;
        let chr_mode = self.bank_select & 0x80 != 0;

        let bank_1k = if !chr_mode {
            match addr_usize >> 10 {
                0 => (self.registers[0] & 0xFE) as usize,
                1 => (self.registers[0] | 1) as usize,
                2 => (self.registers[1] & 0xFE) as usize,
                3 => (self.registers[1] | 1) as usize,
                4 => self.registers[2] as usize,
                5 => self.registers[3] as usize,
                6 => self.registers[4] as usize,
                7 => self.registers[5] as usize,
                _ => unreachable!(),
            }
        } else {
            match addr_usize >> 10 {
                0 => self.registers[2] as usize,
                1 => self.registers[3] as usize,
                2 => self.registers[4] as usize,
                3 => self.registers[5] as usize,
                4 => (self.registers[0] & 0xFE) as usize,
                5 => (self.registers[0] | 1) as usize,
                6 => (self.registers[1] & 0xFE) as usize,
                7 => (self.registers[1] | 1) as usize,
                _ => unreachable!(),
            }
        };

        let offset = addr_usize & 0x3FF;
        let index = (bank_1k * 1024 + offset) % self.chr.len();
        self.chr[index] = value;
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }
}

/// MMC2 (Mapper 9, PxROM): CHR latch-based bank switching.
///
/// Used by Punch-Out!! The mapper selects between two CHR banks for each
/// pattern table half based on latches that update when the PPU reads
/// specific tile addresses. This allows animated tiles without CPU
/// involvement — the PPU's own reads trigger the bank switch.
///
/// - PRG: 8K switchable at $8000-$9FFF, three fixed 8K banks at $A000-$FFFF
/// - CHR: Two latch-selected 4K banks per pattern table half
/// - PRG RAM: none
struct Mmc2 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    /// 8K PRG bank for $8000-$9FFF (4 bits)
    prg_bank: u8,
    /// CHR bank when latch 0 = $FD (PPU $0000-$0FFF)
    chr_fd_0: u8,
    /// CHR bank when latch 0 = $FE (PPU $0000-$0FFF)
    chr_fe_0: u8,
    /// CHR bank when latch 1 = $FD (PPU $1000-$1FFF)
    chr_fd_1: u8,
    /// CHR bank when latch 1 = $FE (PPU $1000-$1FFF)
    chr_fe_1: u8,
    /// Latch 0 state: true = $FE, false = $FD
    latch_0_fe: bool,
    /// Latch 1 state: true = $FE, false = $FD
    latch_1_fe: bool,
    /// Mirroring: false = vertical, true = horizontal
    horizontal_mirror: bool,
}

impl Mmc2 {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>) -> Self {
        Self {
            prg_rom,
            chr_rom,
            prg_bank: 0,
            chr_fd_0: 0,
            chr_fe_0: 0,
            chr_fd_1: 0,
            chr_fe_1: 0,
            latch_0_fe: true, // Power-on: latches set to $FE
            latch_1_fe: true,
            horizontal_mirror: false,
        }
    }

    /// Number of 8K PRG banks.
    fn prg_8k_count(&self) -> usize {
        self.prg_rom.len() / 8192
    }

    /// Read a CHR byte and update latches based on the address.
    ///
    /// The latch updates AFTER the byte is fetched, so the triggering
    /// tile itself uses the old bank selection.
    fn chr_read_with_latch(&mut self, addr: u16) -> u8 {
        let addr_usize = (addr & 0x1FFF) as usize;

        // Select bank based on current latch state
        let bank = if addr_usize < 0x1000 {
            if self.latch_0_fe {
                self.chr_fe_0
            } else {
                self.chr_fd_0
            }
        } else if self.latch_1_fe {
            self.chr_fe_1
        } else {
            self.chr_fd_1
        };

        let offset = addr_usize & 0x0FFF;
        let index = (bank as usize * 4096 + offset) % self.chr_rom.len();
        let data = self.chr_rom[index];

        // Update latches AFTER the read
        match addr {
            0x0FD8 => self.latch_0_fe = false,
            0x0FE8 => self.latch_0_fe = true,
            0x1FD8..=0x1FDF => self.latch_1_fe = false,
            0x1FE8..=0x1FEF => self.latch_1_fe = true,
            _ => {}
        }

        data
    }
}

impl Mapper for Mmc2 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0x9FFF => {
                // Switchable 8K bank
                let bank = self.prg_bank as usize % self.prg_8k_count();
                let offset = (addr - 0x8000) as usize;
                self.prg_rom[bank * 8192 + offset]
            }
            0xA000..=0xFFFF => {
                // Fixed: last three 8K banks
                let count = self.prg_8k_count();
                let fixed_start = count.saturating_sub(3);
                let bank_offset = ((addr - 0xA000) as usize) / 8192;
                let bank = (fixed_start + bank_offset) % count;
                let offset = (addr as usize - 0xA000) % 8192;
                self.prg_rom[bank * 8192 + offset]
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0xA000..=0xAFFF => self.prg_bank = value & 0x0F,
            0xB000..=0xBFFF => self.chr_fd_0 = value & 0x1F,
            0xC000..=0xCFFF => self.chr_fe_0 = value & 0x1F,
            0xD000..=0xDFFF => self.chr_fd_1 = value & 0x1F,
            0xE000..=0xEFFF => self.chr_fe_1 = value & 0x1F,
            0xF000..=0xFFFF => self.horizontal_mirror = value & 1 != 0,
            _ => {}
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        self.chr_read_with_latch(addr)
    }

    fn chr_write(&mut self, _addr: u16, _value: u8) {
        // MMC2 uses CHR ROM only — no writes
    }

    fn mirroring(&self) -> Mirroring {
        if self.horizontal_mirror {
            Mirroring::Horizontal
        } else {
            Mirroring::Vertical
        }
    }
}

/// Color Dreams (Mapper 11): Simple PRG + CHR bank switching.
///
/// Used by unlicensed Color Dreams games (Crystal Mines, Bible Adventures).
///
/// - PRG: 32K switchable at $8000-$FFFF (bits 0-1 of bank register)
/// - CHR: 8K switchable (bits 4-7 of bank register)
/// - Mirroring: fixed from header
struct ColorDreams {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: Mirroring,
    prg_bank: u8,
    chr_bank: u8,
}

impl ColorDreams {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self {
            prg_rom,
            chr_rom,
            mirroring,
            prg_bank: 0,
            chr_bank: 0,
        }
    }
}

impl Mapper for ColorDreams {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => {
                let bank_offset = self.prg_bank as usize * 32768;
                let index = (bank_offset + (addr as usize - 0x8000)) % self.prg_rom.len();
                self.prg_rom[index]
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            self.prg_bank = value & 0x03;
            self.chr_bank = (value >> 4) & 0x0F;
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        let bank_offset = self.chr_bank as usize * 8192;
        let index = (bank_offset + (addr as usize & 0x1FFF)) % self.chr_rom.len().max(1);
        self.chr_rom.get(index).copied().unwrap_or(0)
    }

    fn chr_write(&mut self, _addr: u16, _value: u8) {}

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

/// GxROM (Mapper 66): Simple PRG + CHR bank switching.
///
/// Used by Super Mario Bros / Duck Hunt multicart, Dragon Power.
///
/// - PRG: 32K switchable at $8000-$FFFF (bits 4-5 of bank register)
/// - CHR: 8K switchable (bits 0-1 of bank register)
/// - Mirroring: fixed from header
struct GxRom {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: Mirroring,
    prg_bank: u8,
    chr_bank: u8,
}

impl GxRom {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self {
            prg_rom,
            chr_rom,
            mirroring,
            prg_bank: 0,
            chr_bank: 0,
        }
    }
}

impl Mapper for GxRom {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => {
                let bank_offset = self.prg_bank as usize * 32768;
                let index = (bank_offset + (addr as usize - 0x8000)) % self.prg_rom.len();
                self.prg_rom[index]
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            self.prg_bank = (value >> 4) & 0x03;
            self.chr_bank = value & 0x03;
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        let bank_offset = self.chr_bank as usize * 8192;
        let index = (bank_offset + (addr as usize & 0x1FFF)) % self.chr_rom.len().max(1);
        self.chr_rom.get(index).copied().unwrap_or(0)
    }

    fn chr_write(&mut self, _addr: u16, _value: u8) {}

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

/// MMC4 (Mapper 10, FxROM): CHR latch-based bank switching.
///
/// Used by Fire Emblem and Fire Emblem Gaiden. Nearly identical to MMC2
/// but with 16K PRG banking instead of 8K.
///
/// - PRG: 16K switchable at $8000-$BFFF, 16K fixed (last bank) at $C000-$FFFF
/// - CHR: Two latch-selected 4K banks per pattern table half (same as MMC2)
struct Mmc4 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    prg_bank: u8,
    chr_fd_0: u8,
    chr_fe_0: u8,
    chr_fd_1: u8,
    chr_fe_1: u8,
    latch_0_fe: bool,
    latch_1_fe: bool,
    horizontal_mirror: bool,
}

impl Mmc4 {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>) -> Self {
        Self {
            prg_rom,
            chr_rom,
            prg_bank: 0,
            chr_fd_0: 0,
            chr_fe_0: 0,
            chr_fd_1: 0,
            chr_fe_1: 0,
            latch_0_fe: true,
            latch_1_fe: true,
            horizontal_mirror: false,
        }
    }

    fn prg_16k_count(&self) -> usize {
        self.prg_rom.len() / 16384
    }

    fn chr_read_with_latch(&mut self, addr: u16) -> u8 {
        let addr_usize = (addr & 0x1FFF) as usize;
        let bank = if addr_usize < 0x1000 {
            if self.latch_0_fe { self.chr_fe_0 } else { self.chr_fd_0 }
        } else if self.latch_1_fe {
            self.chr_fe_1
        } else {
            self.chr_fd_1
        };
        let offset = addr_usize & 0x0FFF;
        let index = (bank as usize * 4096 + offset) % self.chr_rom.len().max(1);
        let data = self.chr_rom.get(index).copied().unwrap_or(0);
        // Latch triggers — same addresses as MMC2
        match addr {
            0x0FD8..=0x0FDF => self.latch_0_fe = false,
            0x0FE8..=0x0FEF => self.latch_0_fe = true,
            0x1FD8..=0x1FDF => self.latch_1_fe = false,
            0x1FE8..=0x1FEF => self.latch_1_fe = true,
            _ => {}
        }
        data
    }
}

impl Mapper for Mmc4 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xBFFF => {
                let bank = self.prg_bank as usize % self.prg_16k_count();
                let offset = (addr - 0x8000) as usize;
                self.prg_rom[bank * 16384 + offset]
            }
            0xC000..=0xFFFF => {
                let last = self.prg_16k_count().saturating_sub(1);
                let offset = (addr - 0xC000) as usize;
                self.prg_rom[last * 16384 + offset]
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0xA000..=0xAFFF => self.prg_bank = value & 0x0F,
            0xB000..=0xBFFF => self.chr_fd_0 = value & 0x1F,
            0xC000..=0xCFFF => self.chr_fe_0 = value & 0x1F,
            0xD000..=0xDFFF => self.chr_fd_1 = value & 0x1F,
            0xE000..=0xEFFF => self.chr_fe_1 = value & 0x1F,
            0xF000..=0xFFFF => self.horizontal_mirror = value & 1 != 0,
            _ => {}
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        self.chr_read_with_latch(addr)
    }

    fn chr_write(&mut self, _addr: u16, _value: u8) {}

    fn mirroring(&self) -> Mirroring {
        if self.horizontal_mirror { Mirroring::Horizontal } else { Mirroring::Vertical }
    }
}

/// BxROM (Mapper 34): Simple 32K PRG bank switching.
///
/// Used by Deadly Towers, Impossible Mission II.
///
/// - PRG: 32K switchable at $8000-$FFFF
/// - CHR: 8K RAM
/// - Mirroring: fixed from header
struct BxRom {
    prg_rom: Vec<u8>,
    chr_ram: [u8; 8192],
    mirroring: Mirroring,
    prg_bank: u8,
}

impl BxRom {
    fn new(prg_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self { prg_rom, chr_ram: [0; 8192], mirroring, prg_bank: 0 }
    }
}

impl Mapper for BxRom {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => {
                let bank_offset = self.prg_bank as usize * 32768;
                let index = (bank_offset + (addr as usize - 0x8000)) % self.prg_rom.len();
                self.prg_rom[index]
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            self.prg_bank = value & self.cpu_read(addr);
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        self.chr_ram[(addr as usize) & 0x1FFF]
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        self.chr_ram[(addr as usize) & 0x1FFF] = value;
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

/// Camerica (Mapper 71, Codemasters): 16K PRG bank switching.
///
/// Used by Micro Machines, Fire Hawk, Bee 52, and other Codemasters games.
///
/// - PRG: 16K switchable at $8000-$BFFF, 16K fixed (last bank) at $C000-$FFFF
/// - CHR: 8K RAM
/// - Mirroring: fixed or switchable via $9000-$9FFF (bit 4)
struct Camerica {
    prg_rom: Vec<u8>,
    chr_ram: [u8; 8192],
    mirroring: Mirroring,
    prg_bank: u8,
}

impl Camerica {
    fn new(prg_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self { prg_rom, chr_ram: [0; 8192], mirroring, prg_bank: 0 }
    }

    fn prg_16k_count(&self) -> usize {
        self.prg_rom.len() / 16384
    }
}

impl Mapper for Camerica {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xBFFF => {
                let bank = self.prg_bank as usize % self.prg_16k_count();
                let offset = (addr - 0x8000) as usize;
                self.prg_rom[bank * 16384 + offset]
            }
            0xC000..=0xFFFF => {
                let last = self.prg_16k_count().saturating_sub(1);
                let offset = (addr - 0xC000) as usize;
                self.prg_rom[last * 16384 + offset]
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x9000..=0x9FFF => {
                // Mirroring control (bit 4): 0 = single-screen low, 1 = single-screen high
                self.mirroring = if value & 0x10 != 0 {
                    Mirroring::SingleScreenUpper
                } else {
                    Mirroring::SingleScreenLower
                };
            }
            0xC000..=0xFFFF => {
                self.prg_bank = value;
            }
            _ => {}
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        self.chr_ram[(addr as usize) & 0x1FFF]
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        self.chr_ram[(addr as usize) & 0x1FFF] = value;
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

/// Parse an iNES file and return a boxed mapper.
///
/// # Errors
///
/// Returns an error string if the header is invalid or the mapper is unsupported.
pub fn parse_ines(data: &[u8]) -> Result<Box<dyn Mapper>, String> {
    if data.len() < 16 {
        return Err("iNES file too short (< 16 bytes)".to_string());
    }

    // Check magic: "NES\x1A"
    if &data[0..4] != b"NES\x1a" {
        return Err("Invalid iNES magic (expected NES\\x1A)".to_string());
    }

    let prg_banks = data[4];
    let chr_banks = data[5];
    let flags6 = data[6];
    let flags7 = data[7];

    let mapper_lo = (flags6 >> 4) & 0x0F;
    let mapper_hi = flags7 & 0xF0;
    let mapper_number = mapper_hi | mapper_lo;

    let mirroring = if flags6 & 0x08 != 0 {
        Mirroring::FourScreen
    } else if flags6 & 0x01 != 0 {
        Mirroring::Vertical
    } else {
        Mirroring::Horizontal
    };

    let has_battery = flags6 & 0x02 != 0;
    let has_trainer = flags6 & 0x04 != 0;

    let header = CartridgeHeader {
        prg_rom_banks: prg_banks,
        chr_rom_banks: chr_banks,
        mapper_number,
        mirroring,
        has_battery,
    };

    let prg_size = usize::from(prg_banks) * 16384;
    let chr_size = usize::from(chr_banks) * 8192;

    let prg_start = if has_trainer { 16 + 512 } else { 16 };
    let chr_start = prg_start + prg_size;

    if data.len() < chr_start + chr_size {
        return Err(format!(
            "iNES file too short: expected {} bytes, got {}",
            chr_start + chr_size,
            data.len()
        ));
    }

    let prg_rom = data[prg_start..prg_start + prg_size].to_vec();
    let chr_data = if chr_size > 0 {
        data[chr_start..chr_start + chr_size].to_vec()
    } else {
        Vec::new() // CHR RAM
    };

    match header.mapper_number {
        0 => Ok(Box::new(Nrom::new(prg_rom, chr_data, mirroring))),
        1 => Ok(Box::new(Mmc1::new(prg_rom, chr_data))),
        2 => Ok(Box::new(UxRom::new(prg_rom, chr_data, mirroring))),
        3 => Ok(Box::new(CnRom::new(prg_rom, chr_data, mirroring))),
        4 => Ok(Box::new(Mmc3::new(prg_rom, chr_data))),
        7 => Ok(Box::new(AxRom::new(prg_rom))),
        9 => Ok(Box::new(Mmc2::new(prg_rom, chr_data))),
        10 => Ok(Box::new(Mmc4::new(prg_rom, chr_data))),
        11 => Ok(Box::new(ColorDreams::new(prg_rom, chr_data, mirroring))),
        34 => Ok(Box::new(BxRom::new(prg_rom, mirroring))),
        66 => Ok(Box::new(GxRom::new(prg_rom, chr_data, mirroring))),
        71 => Ok(Box::new(Camerica::new(prg_rom, mirroring))),
        n => Err(format!("Unsupported mapper: {n}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ines(prg_banks: u8, chr_banks: u8, flags6: u8) -> Vec<u8> {
        let prg_size = usize::from(prg_banks) * 16384;
        let chr_size = usize::from(chr_banks) * 8192;
        let mut data = vec![0u8; 16 + prg_size + chr_size];
        data[0..4].copy_from_slice(b"NES\x1a");
        data[4] = prg_banks;
        data[5] = chr_banks;
        data[6] = flags6;
        // Fill PRG with a recognizable pattern
        for i in 0..prg_size {
            data[16 + i] = (i & 0xFF) as u8;
        }
        // Fill CHR with a different pattern
        for i in 0..chr_size {
            data[16 + prg_size + i] = ((i + 0x80) & 0xFF) as u8;
        }
        data
    }

    #[test]
    fn parse_valid_nrom_16k() {
        let data = make_ines(1, 1, 0x00); // Horizontal mirroring
        let mapper = parse_ines(&data).expect("parse failed");
        assert_eq!(mapper.mirroring(), Mirroring::Horizontal);
        // PRG at $8000 should be byte 0 of PRG ROM
        assert_eq!(mapper.cpu_read(0x8000), 0x00);
        // 16K mirrored: $C000 should mirror $8000
        assert_eq!(mapper.cpu_read(0xC000), 0x00);
    }

    #[test]
    fn parse_valid_nrom_32k() {
        let data = make_ines(2, 1, 0x01); // Vertical mirroring
        let mapper = parse_ines(&data).expect("parse failed");
        assert_eq!(mapper.mirroring(), Mirroring::Vertical);
        assert_eq!(mapper.cpu_read(0x8000), 0x00);
        // $C000 maps to bank 1 start
        assert_eq!(mapper.cpu_read(0xC000), 0x00); // offset 0x4000 & 0xFF = 0
    }

    #[test]
    fn chr_read_write_ram() {
        let data = make_ines(1, 0, 0x00); // CHR RAM (0 banks)
        let mut mapper = parse_ines(&data).expect("parse failed");
        assert_eq!(mapper.chr_read(0x0000), 0);
        mapper.chr_write(0x0000, 0xAB);
        assert_eq!(mapper.chr_read(0x0000), 0xAB);
    }

    #[test]
    fn chr_rom_not_writable() {
        let data = make_ines(1, 1, 0x00); // CHR ROM (1 bank)
        let mut mapper = parse_ines(&data).expect("parse failed");
        let original = mapper.chr_read(0x0000);
        mapper.chr_write(0x0000, 0xFF);
        assert_eq!(mapper.chr_read(0x0000), original); // Unchanged
    }

    #[test]
    fn invalid_magic() {
        let data = vec![0u8; 32];
        assert!(parse_ines(&data).is_err());
    }

    #[test]
    fn unsupported_mapper() {
        let mut data = make_ines(1, 1, 0x00);
        data[6] = 0x50; // Mapper 5 (low nibble)
        assert!(parse_ines(&data).is_err());
    }

    // --- MMC1 tests ---

    /// Build an MMC1 with `prg_banks` x 16K PRG and `chr_banks` x 8K CHR.
    /// PRG banks are filled with their bank index (0,1,2…) in every byte.
    /// CHR 4K pages are filled with their page index.
    fn make_mmc1(prg_banks: u8, chr_banks: u8) -> Mmc1 {
        let prg_size = prg_banks as usize * 16384;
        let chr_size = chr_banks as usize * 8192;
        let mut prg_rom = vec![0u8; prg_size];
        for bank in 0..prg_banks as usize {
            for i in 0..16384 {
                prg_rom[bank * 16384 + i] = bank as u8;
            }
        }
        let chr_data = if chr_size > 0 {
            let mut chr = vec![0u8; chr_size];
            let pages = chr_size / 4096;
            for page in 0..pages {
                for i in 0..4096 {
                    chr[page * 4096 + i] = page as u8;
                }
            }
            chr
        } else {
            Vec::new()
        };
        Mmc1::new(prg_rom, chr_data)
    }

    /// Write a 5-bit value to an MMC1 register at `addr`.
    fn mmc1_write_5(mapper: &mut Mmc1, addr: u16, value: u8) {
        for bit in 0..5 {
            mapper.cpu_write(addr, (value >> bit) & 1);
        }
    }

    #[test]
    fn mmc1_parse_ines() {
        // flags6 low nibble = 1 (mapper 1)
        let data = make_ines(8, 2, 0x10);
        let mapper = parse_ines(&data).expect("parse failed");
        // Should be MMC1 — default mirroring from control=0x0C → bits 1:0 = 0
        // = SingleScreenLower
        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenLower);
    }

    #[test]
    fn mmc1_reset_on_bit7() {
        let mut m = make_mmc1(8, 2);
        // Partially fill shift register
        m.cpu_write(0x8000, 1);
        m.cpu_write(0x8000, 0);
        assert_eq!(m.shift_count, 2);
        // Write with bit 7 → reset
        m.cpu_write(0x8000, 0x80);
        assert_eq!(m.shift_count, 0);
        assert_eq!(m.shift_register, 0);
        // PRG mode should be 3 (bits 3:2 of control set)
        assert_eq!((m.control >> 2) & 0x03, 3);
    }

    #[test]
    fn mmc1_shift_register_5_writes() {
        let mut m = make_mmc1(8, 2);
        // Write 0b10101 = 21 to control register ($8000-$9FFF)
        mmc1_write_5(&mut m, 0x8000, 0b10101);
        assert_eq!(m.control, 0b10101);

        // Write 0b00011 = 3 to CHR bank 0 ($A000-$BFFF)
        mmc1_write_5(&mut m, 0xA000, 3);
        assert_eq!(m.chr_bank_0, 3);

        // Write 0b00101 = 5 to CHR bank 1 ($C000-$DFFF)
        mmc1_write_5(&mut m, 0xC000, 5);
        assert_eq!(m.chr_bank_1, 5);

        // Write 0b00010 = 2 to PRG bank ($E000-$FFFF)
        mmc1_write_5(&mut m, 0xE000, 2);
        assert_eq!(m.prg_bank, 2);
    }

    #[test]
    fn mmc1_prg_mode_3_fix_last() {
        // Default mode 3: $8000 = selected bank, $C000 = last bank
        let mut m = make_mmc1(8, 0); // 8 x 16K banks (0-7)
        // control defaults to 0x0C = mode 3
        assert_eq!((m.control >> 2) & 0x03, 3);

        // Select bank 2 for $8000
        mmc1_write_5(&mut m, 0xE000, 2);
        assert_eq!(m.cpu_read(0x8000), 2); // bank 2
        assert_eq!(m.cpu_read(0xC000), 7); // last bank
    }

    #[test]
    fn mmc1_prg_mode_2_fix_first() {
        let mut m = make_mmc1(8, 0);
        // Set mode 2: control bits 3:2 = 10
        mmc1_write_5(&mut m, 0x8000, 0b01000); // control = 8

        // Select bank 5 for $C000
        mmc1_write_5(&mut m, 0xE000, 5);
        assert_eq!(m.cpu_read(0x8000), 0); // bank 0 fixed
        assert_eq!(m.cpu_read(0xC000), 5); // selected bank
    }

    #[test]
    fn mmc1_prg_mode_0_32k() {
        let mut m = make_mmc1(8, 0); // 8 x 16K = 4 x 32K
        // Set mode 0: control bits 3:2 = 00
        mmc1_write_5(&mut m, 0x8000, 0b00000);

        // Select bank 3 — bit 0 ignored in 32K mode, so effective = bank 2
        mmc1_write_5(&mut m, 0xE000, 3);
        // 32K block 1: banks 2,3
        assert_eq!(m.cpu_read(0x8000), 2); // first 16K of 32K block
        assert_eq!(m.cpu_read(0xC000), 3); // second 16K of 32K block
    }

    #[test]
    fn mmc1_chr_4k_mode() {
        let mut m = make_mmc1(2, 2); // 2 x 8K CHR = 4 x 4K pages (0-3)
        // Set CHR 4K mode: control bit 4 = 1
        mmc1_write_5(&mut m, 0x8000, 0b11100); // control = 0x1C

        // Select page 1 for $0000-$0FFF
        mmc1_write_5(&mut m, 0xA000, 1);
        // Select page 3 for $1000-$1FFF
        mmc1_write_5(&mut m, 0xC000, 3);

        assert_eq!(m.chr_read(0x0000), 1); // page 1
        assert_eq!(m.chr_read(0x1000), 3); // page 3
    }

    #[test]
    fn mmc1_chr_8k_mode() {
        let mut m = make_mmc1(2, 2); // 4 x 4K CHR pages
        // Set CHR 8K mode: control bit 4 = 0
        mmc1_write_5(&mut m, 0x8000, 0b01100); // control = 0x0C, CHR 8K

        // Select bank 3 — bit 0 ignored → effective bank 2 (pages 2,3)
        mmc1_write_5(&mut m, 0xA000, 3);
        assert_eq!(m.chr_read(0x0000), 2); // page 2
        assert_eq!(m.chr_read(0x1000), 3); // page 3
    }

    #[test]
    fn mmc1_prg_ram() {
        let mut m = make_mmc1(2, 0);
        // Write and read PRG RAM at $6000-$7FFF
        assert_eq!(m.cpu_read(0x6000), 0);
        m.cpu_write(0x6000, 0x42);
        assert_eq!(m.cpu_read(0x6000), 0x42);
        m.cpu_write(0x7FFF, 0xAB);
        assert_eq!(m.cpu_read(0x7FFF), 0xAB);
    }

    #[test]
    fn mmc1_mirroring_dynamic() {
        let mut m = make_mmc1(2, 0);
        // Default control = 0x0C → bits 1:0 = 0 → SingleScreenLower
        assert_eq!(m.mirroring(), Mirroring::SingleScreenLower);

        // Set mirroring to vertical (bits 1:0 = 2)
        mmc1_write_5(&mut m, 0x8000, 0b01110); // control = 0x0E
        assert_eq!(m.mirroring(), Mirroring::Vertical);

        // Set to horizontal (bits 1:0 = 3)
        mmc1_write_5(&mut m, 0x8000, 0b01111); // control = 0x0F
        assert_eq!(m.mirroring(), Mirroring::Horizontal);

        // Set to single-screen upper (bits 1:0 = 1)
        mmc1_write_5(&mut m, 0x8000, 0b01101); // control = 0x0D
        assert_eq!(m.mirroring(), Mirroring::SingleScreenUpper);
    }

    // --- UxROM tests ---

    #[test]
    fn uxrom_parse_ines() {
        // Mapper 2: flags6 high nibble = 0x2_, so flags6 = 0x20
        let data = make_ines(8, 0, 0x20);
        let mapper = parse_ines(&data).expect("parse failed");
        assert_eq!(mapper.mirroring(), Mirroring::Horizontal);
    }

    #[test]
    fn uxrom_prg_switching() {
        // 8 x 16K PRG banks. $C000 fixed to last bank.
        // Each bank filled with its number; first byte = $FF for bus-conflict-safe writes.
        let mut m = UxRom::new(
            {
                let mut prg = vec![0u8; 8 * 16384];
                for bank in 0..8usize {
                    for i in 0..16384 {
                        prg[bank * 16384 + i] = bank as u8;
                    }
                    prg[bank * 16384] = 0xFF; // bus-conflict-safe write target
                }
                prg
            },
            Vec::new(),
            Mirroring::Vertical,
        );

        // Default: bank 0 at $8000, last bank at $C000
        assert_eq!(m.cpu_read(0x8001), 0); // byte 1, not 0 (byte 0 is $FF)
        assert_eq!(m.cpu_read(0xC001), 7);

        // Switch to bank 3 (write to $8000 where ROM=$FF, so 3&$FF=3)
        m.cpu_write(0x8000, 3);
        assert_eq!(m.cpu_read(0x8001), 3);
        assert_eq!(m.cpu_read(0xC001), 7); // Still last bank
    }

    #[test]
    fn uxrom_chr_ram() {
        let mut m = UxRom::new(vec![0u8; 16384], Vec::new(), Mirroring::Horizontal);
        assert_eq!(m.chr_read(0x0000), 0);
        m.chr_write(0x0000, 0xAB);
        assert_eq!(m.chr_read(0x0000), 0xAB);
    }

    #[test]
    fn uxrom_fixed_mirroring() {
        let m = UxRom::new(vec![0u8; 16384], Vec::new(), Mirroring::Vertical);
        assert_eq!(m.mirroring(), Mirroring::Vertical);
    }

    // --- CNROM tests ---

    #[test]
    fn cnrom_parse_ines() {
        // Mapper 3: flags6 high nibble = 0x3_, so flags6 = 0x30
        let data = make_ines(2, 4, 0x30);
        let mapper = parse_ines(&data).expect("parse failed");
        assert_eq!(mapper.mirroring(), Mirroring::Horizontal);
    }

    #[test]
    fn cnrom_prg_unbanked_32k() {
        let m = CnRom::new(
            {
                let mut prg = vec![0u8; 32768];
                prg[0] = 0xAA;
                prg[0x4000] = 0xBB;
                prg
            },
            vec![0u8; 32768],
            Mirroring::Vertical,
        );
        assert_eq!(m.cpu_read(0x8000), 0xAA);
        assert_eq!(m.cpu_read(0xC000), 0xBB);
    }

    #[test]
    fn cnrom_prg_unbanked_16k_mirrored() {
        let m = CnRom::new(
            {
                let mut prg = vec![0u8; 16384];
                prg[0] = 0xCC;
                prg
            },
            vec![0u8; 32768],
            Mirroring::Horizontal,
        );
        // 16K mirrored: $8000 and $C000 both see offset 0
        assert_eq!(m.cpu_read(0x8000), 0xCC);
        assert_eq!(m.cpu_read(0xC000), 0xCC);
    }

    #[test]
    fn cnrom_chr_switching() {
        // 4 x 8K CHR banks, each filled with bank index
        let mut chr = vec![0u8; 4 * 8192];
        for bank in 0..4usize {
            for i in 0..8192 {
                chr[bank * 8192 + i] = bank as u8;
            }
        }
        let mut m = CnRom::new(vec![0xFFu8; 32768], chr, Mirroring::Vertical);

        // Default: bank 0
        assert_eq!(m.chr_read(0x0000), 0);

        // Switch to bank 2
        m.cpu_write(0x8000, 2);
        assert_eq!(m.chr_read(0x0000), 2);

        // Switch to bank 3
        m.cpu_write(0xFFFF, 3);
        assert_eq!(m.chr_read(0x0000), 3);
    }

    #[test]
    fn cnrom_chr_not_writable() {
        let mut m = CnRom::new(vec![0u8; 32768], vec![0u8; 8192], Mirroring::Vertical);
        let original = m.chr_read(0x0000);
        m.chr_write(0x0000, 0xFF);
        assert_eq!(m.chr_read(0x0000), original);
    }

    // --- MMC2 tests ---

    /// Build an MMC2 with `prg_8k_banks` x 8K PRG and `chr_4k_pages` x 4K CHR.
    /// PRG 8K banks are filled with their bank index.
    /// CHR 4K pages are filled with their page index.
    fn make_mmc2(prg_8k_banks: u8, chr_4k_pages: u8) -> Mmc2 {
        let prg_size = prg_8k_banks as usize * 8192;
        let chr_size = chr_4k_pages as usize * 4096;
        let mut prg_rom = vec![0u8; prg_size];
        for bank in 0..prg_8k_banks as usize {
            for i in 0..8192 {
                prg_rom[bank * 8192 + i] = bank as u8;
            }
        }
        let mut chr_rom = vec![0u8; chr_size];
        for page in 0..chr_4k_pages as usize {
            for i in 0..4096 {
                chr_rom[page * 4096 + i] = page as u8;
            }
        }
        Mmc2::new(prg_rom, chr_rom)
    }

    #[test]
    fn mmc2_parse_ines() {
        // Mapper 9: flags6 high nibble = 0x9_, so flags6 = 0x90
        let data = make_ines(8, 2, 0x90);
        let mapper = parse_ines(&data).expect("parse failed");
        // Default mirroring is vertical
        assert_eq!(mapper.mirroring(), Mirroring::Vertical);
    }

    #[test]
    fn mmc2_prg_banking() {
        // 16 x 8K PRG banks. Last three ($A000-$FFFF) are fixed.
        let mut m = make_mmc2(16, 8);

        // Default: bank 0 at $8000
        assert_eq!(m.cpu_read(0x8000), 0);
        // $A000 = bank 13, $C000 = bank 14, $E000 = bank 15 (last three)
        assert_eq!(m.cpu_read(0xA000), 13);
        assert_eq!(m.cpu_read(0xC000), 14);
        assert_eq!(m.cpu_read(0xE000), 15);

        // Switch $8000 to bank 5
        m.cpu_write(0xA000, 5);
        assert_eq!(m.cpu_read(0x8000), 5);
        // Fixed banks unchanged
        assert_eq!(m.cpu_read(0xA000), 13);
    }

    #[test]
    fn mmc2_chr_latch_default() {
        // 8 x 4K CHR pages. Latches power on as $FE.
        let mut m = make_mmc2(16, 8);

        // Set $FD banks to page 1 ($0000) and page 3 ($1000)
        m.cpu_write(0xB000, 1); // chr_fd_0
        m.cpu_write(0xD000, 3); // chr_fd_1
        // Set $FE banks to page 2 ($0000) and page 5 ($1000)
        m.cpu_write(0xC000, 2); // chr_fe_0
        m.cpu_write(0xE000, 5); // chr_fe_1

        // Latches default to $FE, so should read FE banks
        assert_eq!(m.chr_read(0x0000), 2); // chr_fe_0 = page 2
        assert_eq!(m.chr_read(0x1000), 5); // chr_fe_1 = page 5
    }

    #[test]
    fn mmc2_latch_0_fd_trigger() {
        let mut m = make_mmc2(16, 8);
        m.cpu_write(0xB000, 1); // chr_fd_0 = page 1
        m.cpu_write(0xC000, 2); // chr_fe_0 = page 2

        // Latch defaults to $FE → reads page 2
        assert_eq!(m.chr_read(0x0000), 2);

        // Read $0FD8 → triggers latch 0 to $FD (AFTER the read)
        let val = m.chr_read(0x0FD8);
        assert_eq!(val, 2); // Still reads from old $FE bank

        // Now latch 0 = $FD → reads page 1
        assert_eq!(m.chr_read(0x0000), 1);
    }

    #[test]
    fn mmc2_latch_0_fe_trigger() {
        let mut m = make_mmc2(16, 8);
        m.cpu_write(0xB000, 1); // chr_fd_0 = page 1
        m.cpu_write(0xC000, 2); // chr_fe_0 = page 2

        // Force latch 0 to $FD first
        m.chr_read(0x0FD8);
        assert_eq!(m.chr_read(0x0000), 1); // Confirms $FD

        // Read $0FE8 → triggers latch 0 back to $FE
        m.chr_read(0x0FE8);
        assert_eq!(m.chr_read(0x0000), 2); // Back to $FE bank
    }

    #[test]
    fn mmc2_latch_1_fd_trigger() {
        let mut m = make_mmc2(16, 8);
        m.cpu_write(0xD000, 3); // chr_fd_1 = page 3
        m.cpu_write(0xE000, 5); // chr_fe_1 = page 5

        // Latch 1 defaults to $FE
        assert_eq!(m.chr_read(0x1000), 5);

        // Read in $1FD8-$1FDF range → latch 1 to $FD
        m.chr_read(0x1FD8);
        assert_eq!(m.chr_read(0x1000), 3); // Now $FD bank

        // Also test $1FDF (end of range)
        // Reset to $FE first
        m.chr_read(0x1FE8);
        assert_eq!(m.chr_read(0x1000), 5);
        m.chr_read(0x1FDF);
        assert_eq!(m.chr_read(0x1000), 3);
    }

    #[test]
    fn mmc2_latch_1_fe_trigger() {
        let mut m = make_mmc2(16, 8);
        m.cpu_write(0xD000, 3); // chr_fd_1 = page 3
        m.cpu_write(0xE000, 5); // chr_fe_1 = page 5

        // Force latch 1 to $FD
        m.chr_read(0x1FD8);
        assert_eq!(m.chr_read(0x1000), 3);

        // Read in $1FE8-$1FEF range → latch 1 to $FE
        m.chr_read(0x1FEF);
        assert_eq!(m.chr_read(0x1000), 5);
    }

    #[test]
    fn mmc2_mirroring() {
        let mut m = make_mmc2(16, 8);
        // Default: vertical
        assert_eq!(m.mirroring(), Mirroring::Vertical);

        // Set horizontal
        m.cpu_write(0xF000, 1);
        assert_eq!(m.mirroring(), Mirroring::Horizontal);

        // Set vertical
        m.cpu_write(0xF000, 0);
        assert_eq!(m.mirroring(), Mirroring::Vertical);
    }

    #[test]
    fn mmc2_chr_rom_not_writable() {
        let mut m = make_mmc2(16, 8);
        let original = m.chr_read(0x0000);
        m.chr_write(0x0000, 0xFF);
        assert_eq!(m.chr_read(0x0000), original);
    }

    #[test]
    fn mmc2_latches_independent() {
        // Latch 0 and latch 1 don't affect each other
        let mut m = make_mmc2(16, 8);
        m.cpu_write(0xB000, 1); // chr_fd_0
        m.cpu_write(0xC000, 2); // chr_fe_0
        m.cpu_write(0xD000, 3); // chr_fd_1
        m.cpu_write(0xE000, 5); // chr_fe_1

        // Toggle latch 0 to $FD — latch 1 should stay at $FE
        m.chr_read(0x0FD8);
        assert_eq!(m.chr_read(0x0000), 1); // latch 0 = $FD
        assert_eq!(m.chr_read(0x1000), 5); // latch 1 still $FE

        // Toggle latch 1 to $FD — latch 0 should stay at $FD
        m.chr_read(0x1FD8);
        assert_eq!(m.chr_read(0x0000), 1); // latch 0 still $FD
        assert_eq!(m.chr_read(0x1000), 3); // latch 1 = $FD
    }

    // --- MMC3 tests ---

    /// Build an MMC3 with `prg_8k_banks` x 8K PRG and `chr_1k_pages` x 1K CHR.
    /// PRG 8K banks filled with their bank index. CHR 1K pages filled with page index.
    fn make_mmc3(prg_8k_banks: usize, chr_1k_pages: usize) -> Mmc3 {
        let prg_size = prg_8k_banks * 8192;
        let chr_size = chr_1k_pages * 1024;
        let mut prg_rom = vec![0u8; prg_size];
        for bank in 0..prg_8k_banks {
            for i in 0..8192 {
                prg_rom[bank * 8192 + i] = bank as u8;
            }
        }
        let chr_data = if chr_size > 0 {
            let mut chr = vec![0u8; chr_size];
            for page in 0..chr_1k_pages {
                for i in 0..1024 {
                    chr[page * 1024 + i] = page as u8;
                }
            }
            chr
        } else {
            Vec::new()
        };
        Mmc3::new(prg_rom, chr_data)
    }

    #[test]
    fn mmc3_parse_ines() {
        // Mapper 4: flags6 high nibble = 0x4_, so flags6 = 0x40
        let data = make_ines(8, 4, 0x40);
        let mapper = parse_ines(&data).expect("parse failed");
        // MMC3 default mirroring is vertical
        assert_eq!(mapper.mirroring(), Mirroring::Vertical);
    }

    #[test]
    fn mmc3_prg_mode_0() {
        // 32 x 8K PRG banks. Mode 0: R6@$8000, R7@$A000, -2@$C000, -1@$E000
        let mut m = make_mmc3(32, 256);

        // Select register 6, PRG mode 0 (bit 6 clear)
        m.cpu_write(0x8000, 6); // bank_select = 6
        m.cpu_write(0x8001, 5); // R6 = 5

        m.cpu_write(0x8000, 7); // bank_select = 7
        m.cpu_write(0x8001, 10); // R7 = 10

        assert_eq!(m.cpu_read(0x8000), 5);  // R6
        assert_eq!(m.cpu_read(0xA000), 10); // R7
        assert_eq!(m.cpu_read(0xC000), 30); // second-to-last
        assert_eq!(m.cpu_read(0xE000), 31); // last
    }

    #[test]
    fn mmc3_prg_mode_1() {
        // Mode 1 (bit 6 set): -2@$8000, R7@$A000, R6@$C000, -1@$E000
        let mut m = make_mmc3(32, 256);

        // Set PRG mode 1, select R6
        m.cpu_write(0x8000, 0x46); // bank_select = 0x46 (bit 6 set, reg 6)
        m.cpu_write(0x8001, 5);    // R6 = 5

        m.cpu_write(0x8000, 0x47); // reg 7
        m.cpu_write(0x8001, 10);   // R7 = 10

        assert_eq!(m.cpu_read(0x8000), 30); // second-to-last (fixed)
        assert_eq!(m.cpu_read(0xA000), 10); // R7
        assert_eq!(m.cpu_read(0xC000), 5);  // R6 (swapped to $C000)
        assert_eq!(m.cpu_read(0xE000), 31); // last
    }

    #[test]
    fn mmc3_chr_mode_0() {
        // Mode 0: R0,R0+1 (2K) | R1,R1+1 (2K) | R2,R3,R4,R5 (4x1K)
        let mut m = make_mmc3(4, 256);

        // bank_select bit 7 = 0 (mode 0), select R0
        m.cpu_write(0x8000, 0); // reg 0
        m.cpu_write(0x8001, 4); // R0 = 4 (bit 0 ignored → pages 4,5)

        m.cpu_write(0x8000, 1); // reg 1
        m.cpu_write(0x8001, 8); // R1 = 8 (→ pages 8,9)

        m.cpu_write(0x8000, 2); m.cpu_write(0x8001, 20); // R2 = 20
        m.cpu_write(0x8000, 3); m.cpu_write(0x8001, 21); // R3 = 21
        m.cpu_write(0x8000, 4); m.cpu_write(0x8001, 22); // R4 = 22
        m.cpu_write(0x8000, 5); m.cpu_write(0x8001, 23); // R5 = 23

        // $0000-$03FF = page 4, $0400-$07FF = page 5
        assert_eq!(m.chr_read(0x0000), 4);
        assert_eq!(m.chr_read(0x0400), 5);
        // $0800-$0BFF = page 8, $0C00-$0FFF = page 9
        assert_eq!(m.chr_read(0x0800), 8);
        assert_eq!(m.chr_read(0x0C00), 9);
        // $1000-$13FF = page 20, etc.
        assert_eq!(m.chr_read(0x1000), 20);
        assert_eq!(m.chr_read(0x1400), 21);
        assert_eq!(m.chr_read(0x1800), 22);
        assert_eq!(m.chr_read(0x1C00), 23);
    }

    #[test]
    fn mmc3_chr_mode_1() {
        // Mode 1 (bit 7 set): R2,R3,R4,R5 (4x1K) | R0,R0+1 (2K) | R1,R1+1 (2K)
        let mut m = make_mmc3(4, 256);

        // Set CHR mode 1
        m.cpu_write(0x8000, 0x80); // reg 0, chr mode 1
        m.cpu_write(0x8001, 4);    // R0 = 4

        m.cpu_write(0x8000, 0x81); // reg 1
        m.cpu_write(0x8001, 8);    // R1 = 8

        m.cpu_write(0x8000, 0x82); m.cpu_write(0x8001, 20); // R2
        m.cpu_write(0x8000, 0x83); m.cpu_write(0x8001, 21); // R3
        m.cpu_write(0x8000, 0x84); m.cpu_write(0x8001, 22); // R4
        m.cpu_write(0x8000, 0x85); m.cpu_write(0x8001, 23); // R5

        // $0000 = R2, $0400 = R3, $0800 = R4, $0C00 = R5
        assert_eq!(m.chr_read(0x0000), 20);
        assert_eq!(m.chr_read(0x0400), 21);
        assert_eq!(m.chr_read(0x0800), 22);
        assert_eq!(m.chr_read(0x0C00), 23);
        // $1000 = R0 (page 4), $1400 = R0+1 (page 5)
        assert_eq!(m.chr_read(0x1000), 4);
        assert_eq!(m.chr_read(0x1400), 5);
        // $1800 = R1 (page 8), $1C00 = R1+1 (page 9)
        assert_eq!(m.chr_read(0x1800), 8);
        assert_eq!(m.chr_read(0x1C00), 9);
    }

    #[test]
    fn mmc3_mirroring() {
        let mut m = make_mmc3(4, 8);

        // Default: vertical
        assert_eq!(m.mirroring(), Mirroring::Vertical);

        // Switch to horizontal
        m.cpu_write(0xA000, 1);
        assert_eq!(m.mirroring(), Mirroring::Horizontal);

        // Switch back to vertical
        m.cpu_write(0xA000, 0);
        assert_eq!(m.mirroring(), Mirroring::Vertical);
    }

    #[test]
    fn mmc3_prg_ram() {
        let mut m = make_mmc3(4, 8);

        // PRG RAM enabled by default
        assert_eq!(m.cpu_read(0x6000), 0);
        m.cpu_write(0x6000, 0x42);
        assert_eq!(m.cpu_read(0x6000), 0x42);
        m.cpu_write(0x7FFF, 0xAB);
        assert_eq!(m.cpu_read(0x7FFF), 0xAB);

        // Write protect: $A001 with bit 6 set, bit 7 set (enable + protect)
        m.cpu_write(0xA001, 0xC0);
        m.cpu_write(0x6000, 0xFF); // Should be blocked
        assert_eq!(m.cpu_read(0x6000), 0x42); // Unchanged

        // Disable write protect: bit 7 set, bit 6 clear
        m.cpu_write(0xA001, 0x80);
        m.cpu_write(0x6000, 0xFF);
        assert_eq!(m.cpu_read(0x6000), 0xFF); // Written
    }

    #[test]
    fn mmc3_irq_counter() {
        // Scanline counter fires after N+1 A12 rising edges when latch=N.
        let mut m = make_mmc3(4, 256);
        m.irq_enabled = true; // Enable directly for unit test

        // Set latch to 3
        m.cpu_write(0xC000, 3);   // latch = 3
        m.cpu_write(0xC001, 0);   // reload flag set

        // Simulate A12 rising edges by reading from $1000+ (A12=1)
        // after reading from $0000 (A12=0) to create a transition.
        // Edge 1: counter loaded from latch (3), no fire
        m.chr_read(0x0000); // A12 low
        m.chr_read(0x1000); // A12 rising edge → counter = 3
        assert!(!m.irq_pending);

        // Edge 2: counter = 2
        m.chr_read(0x0000);
        m.chr_read(0x1000);
        assert!(!m.irq_pending);

        // Edge 3: counter = 1
        m.chr_read(0x0000);
        m.chr_read(0x1000);
        assert!(!m.irq_pending);

        // Edge 4: counter = 0, IRQ fires
        m.chr_read(0x0000);
        m.chr_read(0x1000);
        assert!(m.irq_pending);
    }

    #[test]
    fn mmc3_irq_disable() {
        let mut m = make_mmc3(4, 256);

        // Force IRQ pending
        m.irq_pending = true;
        m.irq_enabled = true;

        // Write to $E000 (even) → disable + acknowledge
        m.cpu_write(0xE000, 0);
        assert!(!m.irq_pending);
        assert!(!m.irq_enabled);
    }

    #[test]
    fn mmc3_irq_reload() {
        // Writing $C001 causes counter to reload from latch on next clock.
        let mut m = make_mmc3(4, 256);
        m.irq_enabled = true;

        // Set latch to 2
        m.cpu_write(0xC000, 2);
        m.cpu_write(0xC001, 0); // reload flag

        // Clock once → loads latch (2)
        m.chr_read(0x0000);
        m.chr_read(0x1000);
        assert_eq!(m.irq_counter, 2);

        // Clock twice more → counter reaches 0
        m.chr_read(0x0000);
        m.chr_read(0x1000); // 1
        m.chr_read(0x0000);
        m.chr_read(0x1000); // 0 → IRQ
        assert!(m.irq_pending);

        // Now change latch to 5 and trigger reload
        m.irq_pending = false;
        m.cpu_write(0xC000, 5);
        m.cpu_write(0xC001, 0); // set reload flag

        // Next clock reloads from new latch
        m.chr_read(0x0000);
        m.chr_read(0x1000);
        assert_eq!(m.irq_counter, 5);
        assert!(!m.irq_pending);
    }

    // --- AxROM tests ---

    #[test]
    fn axrom_parse_ines() {
        // Mapper 7: flags6 high nibble = 0x7_, so flags6 = 0x70
        let data = make_ines(2, 0, 0x70);
        let mapper = parse_ines(&data).expect("parse failed");
        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenLower);
    }

    #[test]
    fn axrom_prg_switching() {
        // 8 x 32K PRG banks, each filled with bank index.
        // First byte of each bank = $FF for bus-conflict-safe writes.
        let mut prg = vec![0u8; 8 * 32768];
        for bank in 0..8usize {
            for i in 0..32768 {
                prg[bank * 32768 + i] = bank as u8;
            }
            prg[bank * 32768] = 0xFF;
        }
        let mut m = AxRom::new(prg);

        // Default: bank 0
        assert_eq!(m.cpu_read(0x8001), 0);
        assert_eq!(m.cpu_read(0xFFFF), 0);

        // Switch to bank 3 (write to $8000 where ROM=$FF)
        m.cpu_write(0x8000, 3);
        assert_eq!(m.cpu_read(0x8001), 3);
        assert_eq!(m.cpu_read(0xC001), 3);
    }

    #[test]
    fn axrom_mirroring_switch() {
        // Fill PRG with $FF so bus conflict AND is transparent.
        let mut m = AxRom::new(vec![0xFFu8; 32768]);

        // Default: single-screen lower
        assert_eq!(m.mirroring(), Mirroring::SingleScreenLower);

        // Set bit 4 → upper
        m.cpu_write(0x8000, 0x10);
        assert_eq!(m.mirroring(), Mirroring::SingleScreenUpper);

        // Clear bit 4 → lower
        m.cpu_write(0x8000, 0x02);
        assert_eq!(m.mirroring(), Mirroring::SingleScreenLower);
    }

    #[test]
    fn axrom_chr_ram() {
        let mut m = AxRom::new(vec![0u8; 32768]);
        assert_eq!(m.chr_read(0x0000), 0);
        m.chr_write(0x0000, 0xAB);
        assert_eq!(m.chr_read(0x0000), 0xAB);
    }

    // --- Color Dreams (Mapper 11) ---

    #[test]
    fn color_dreams_prg_switching() {
        // 4 × 32K = 128K PRG
        let mut prg = vec![0u8; 4 * 32768];
        prg[0] = 0xAA;          // bank 0, addr $8000
        prg[32768] = 0xBB;      // bank 1, addr $8000
        prg[2 * 32768] = 0xCC;  // bank 2, addr $8000
        let chr = vec![0u8; 8192];
        let mut m = ColorDreams::new(prg, chr, Mirroring::Vertical);

        assert_eq!(m.cpu_read(0x8000), 0xAA); // bank 0 default
        m.cpu_write(0x8000, 0x01); // PRG bank 1
        assert_eq!(m.cpu_read(0x8000), 0xBB);
        m.cpu_write(0x8000, 0x02); // PRG bank 2
        assert_eq!(m.cpu_read(0x8000), 0xCC);
    }

    #[test]
    fn color_dreams_chr_switching() {
        let mut chr = vec![0u8; 4 * 8192];
        chr[0] = 0x11;        // bank 0
        chr[8192] = 0x22;     // bank 1
        chr[2 * 8192] = 0x33; // bank 2
        let prg = vec![0u8; 32768];
        let mut m = ColorDreams::new(prg, chr, Mirroring::Horizontal);

        assert_eq!(m.chr_read(0x0000), 0x11); // bank 0
        m.cpu_write(0x8000, 0x10); // CHR bank 1 (bits 4-7)
        assert_eq!(m.chr_read(0x0000), 0x22);
        m.cpu_write(0x8000, 0x20); // CHR bank 2
        assert_eq!(m.chr_read(0x0000), 0x33);
    }

    #[test]
    fn color_dreams_parse_ines() {
        // Mapper 11 = (0xB0 >> 4) | (flags6 bits 4-7) = 0x10 | 0x01 = 11
        // flags6: low nibble mapper = 0xB0, flags7: high nibble mapper = 0x00
        let data = make_ines(2, 1, 0xB0); // 2×16K PRG + 1×8K CHR, mapper 11
        let m = parse_ines(&data);
        assert!(m.is_ok(), "Mapper 11 should parse successfully");
    }

    // --- GxROM (Mapper 66) ---

    #[test]
    fn gxrom_prg_switching() {
        let mut prg = vec![0u8; 4 * 32768];
        prg[0] = 0xAA;          // bank 0
        prg[32768] = 0xBB;      // bank 1
        prg[2 * 32768] = 0xCC;  // bank 2
        let chr = vec![0u8; 8192];
        let mut m = GxRom::new(prg, chr, Mirroring::Vertical);

        assert_eq!(m.cpu_read(0x8000), 0xAA);
        m.cpu_write(0x8000, 0x10); // PRG bank 1 (bits 4-5)
        assert_eq!(m.cpu_read(0x8000), 0xBB);
        m.cpu_write(0x8000, 0x20); // PRG bank 2
        assert_eq!(m.cpu_read(0x8000), 0xCC);
    }

    #[test]
    fn gxrom_chr_switching() {
        let mut chr = vec![0u8; 4 * 8192];
        chr[0] = 0x11;
        chr[8192] = 0x22;
        chr[2 * 8192] = 0x33;
        let prg = vec![0u8; 32768];
        let mut m = GxRom::new(prg, chr, Mirroring::Horizontal);

        assert_eq!(m.chr_read(0x0000), 0x11);
        m.cpu_write(0x8000, 0x01); // CHR bank 1 (bits 0-1)
        assert_eq!(m.chr_read(0x0000), 0x22);
        m.cpu_write(0x8000, 0x02); // CHR bank 2
        assert_eq!(m.chr_read(0x0000), 0x33);
    }

    #[test]
    fn gxrom_parse_ines() {
        let mut data = make_ines(2, 1, 0x20);
        data[7] = 0x40;
        let m = parse_ines(&data);
        assert!(m.is_ok(), "Mapper 66 should parse successfully");
    }

    // --- MMC4 (Mapper 10) ---

    #[test]
    fn mmc4_prg_16k_switching() {
        let mut prg = vec![0u8; 4 * 16384]; // 4 × 16K
        prg[0] = 0xAA;          // bank 0
        prg[16384] = 0xBB;      // bank 1
        let chr = vec![0u8; 8 * 4096]; // 8 × 4K CHR
        let mut m = Mmc4::new(prg, chr);
        assert_eq!(m.cpu_read(0x8000), 0xAA);
        m.cpu_write(0xA000, 1); // switch to bank 1
        assert_eq!(m.cpu_read(0x8000), 0xBB);
    }

    #[test]
    fn mmc4_fixed_last_bank() {
        let mut prg = vec![0u8; 4 * 16384];
        prg[3 * 16384] = 0xDD; // last bank
        let chr = vec![0u8; 4096];
        let m = Mmc4::new(prg, chr);
        assert_eq!(m.cpu_read(0xC000), 0xDD);
    }

    #[test]
    fn mmc4_chr_latch() {
        let mut chr = vec![0u8; 8 * 4096];
        chr[0] = 0x11;        // bank 0 ($FD)
        chr[4096] = 0x22;     // bank 1 ($FE)
        let prg = vec![0u8; 16384];
        let mut m = Mmc4::new(prg, chr);
        m.cpu_write(0xB000, 0); // FD bank 0 = 0
        m.cpu_write(0xC000, 1); // FE bank 0 = 1
        // Default latch = FE, so should read bank 1
        assert_eq!(m.chr_read(0x0000), 0x22);
    }

    // --- BxROM (Mapper 34) ---

    #[test]
    fn bxrom_prg_switching() {
        let mut prg = vec![0u8; 4 * 32768];
        prg[0] = 0xFF;         // bus-conflict-safe write target
        prg[1] = 0xAA;         // identifying byte at offset 1
        prg[32768] = 0xFF;     // bank 1 write target
        prg[32768 + 1] = 0xBB; // identifying byte
        let mut m = BxRom::new(prg, Mirroring::Vertical);
        assert_eq!(m.cpu_read(0x8001), 0xAA);
        m.cpu_write(0x8000, 1); // write 1, ROM=$FF, so 1&$FF=1
        assert_eq!(m.cpu_read(0x8001), 0xBB);
    }

    #[test]
    fn bxrom_chr_ram() {
        let mut m = BxRom::new(vec![0u8; 32768], Mirroring::Horizontal);
        m.chr_write(0x0000, 0xAB);
        assert_eq!(m.chr_read(0x0000), 0xAB);
    }

    // --- Camerica (Mapper 71) ---

    #[test]
    fn camerica_prg_switching() {
        let mut prg = vec![0u8; 4 * 16384];
        prg[0] = 0xAA;
        prg[16384] = 0xBB;
        let mut m = Camerica::new(prg, Mirroring::Vertical);
        assert_eq!(m.cpu_read(0x8000), 0xAA);
        m.cpu_write(0xC000, 1);
        assert_eq!(m.cpu_read(0x8000), 0xBB);
    }

    #[test]
    fn camerica_fixed_last_bank() {
        let mut prg = vec![0u8; 4 * 16384];
        prg[3 * 16384] = 0xDD;
        let m = Camerica::new(prg, Mirroring::Vertical);
        assert_eq!(m.cpu_read(0xC000), 0xDD);
    }

    #[test]
    fn camerica_mirroring_control() {
        let mut m = Camerica::new(vec![0u8; 32768], Mirroring::Vertical);
        m.cpu_write(0x9000, 0x10);
        assert_eq!(m.mirroring(), Mirroring::SingleScreenUpper);
        m.cpu_write(0x9000, 0x00);
        assert_eq!(m.mirroring(), Mirroring::SingleScreenLower);
    }
}
