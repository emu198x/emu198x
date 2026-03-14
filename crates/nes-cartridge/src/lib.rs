//! iNES cartridge parser and mapper implementations.
//!
//! Parses the iNES file format (header + PRG ROM + CHR ROM) and provides
//! a `Mapper` trait for address translation. Supports 48 mapper numbers
//! covering virtually every licensed NES/Famicom game:
//!
//! NROM (0), MMC1 (1), UxROM (2), CNROM (3), MMC3 (4), MMC5 (5),
//! AxROM (7), MMC2 (9), MMC4 (10), Color Dreams (11), Bandai FCG (16/159),
//! Jaleco SS88006 (18), Namco 163 (19), VRC4a (21), VRC2a (22),
//! VRC2b/VRC4e (23), VRC6a (24), VRC4b (25), VRC6b (26), Irem G-101 (32),
//! Taito TC0190 (33), BxROM (34), Taito TC0690 (48), RAMBO-1 (64),
//! Irem H3001 (65), GxROM (66), Sunsoft-3 (67), Sunsoft-4 (68),
//! Sunsoft FME-7 (69), Bandai 74161 (70), Camerica (71), Jaleco JF-17 (72),
//! VRC1 (75), Irem 74161 (78), NINA-003 (79), Taito X1-005 (80),
//! Taito X1-017 (82), VRC7 (85), Mapper 87, Namco 3446 (88),
//! Sunsoft-2 (93), TxSROM (118), TQROM (119), Jaleco JF-11 (140),
//! Bandai 74161+SS (152), Sunsoft-1 (184), CNROM+protection (185),
//! Mapper 206, and Namco 175/340 (210).
//!
//! Expansion audio is implemented for Sunsoft 5B (mapper 69), VRC6 (24/26),
//! and Namco 163 (19). VRC7 OPLL FM synthesis accepts register writes but
//! does not yet produce audio output.

#![allow(clippy::cast_possible_truncation)]

pub use ricoh_ppu_2c02::Mirroring;

/// Parsed iNES file header.
#[derive(Debug)]
#[allow(dead_code)]
pub struct CartridgeHeader {
    pub prg_rom_banks: u8,
    pub chr_rom_banks: u8,
    pub mapper_number: u16,
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

    /// Expansion audio output level. Added to the APU mixer each CPU cycle.
    /// Range: 0.0 to ~0.5. Default: 0.0 (no expansion audio).
    fn audio_output(&self) -> f32 {
        0.0
    }

    /// Tick the mapper's audio engine one CPU cycle. Called once per CPU
    /// cycle for mappers with expansion audio (Sunsoft 5B, VRC6, Namco 163).
    fn tick_audio(&mut self) {}

    /// Read battery-backed PRG RAM contents. Returns `None` if the mapper
    /// has no PRG RAM (e.g. NROM, `UxROM`).
    fn prg_ram(&self) -> Option<&[u8]> {
        None
    }

    /// Restore battery-backed PRG RAM from a save file. No-op if the
    /// mapper has no PRG RAM.
    fn set_prg_ram(&mut self, _data: &[u8]) {}
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

    fn prg_ram(&self) -> Option<&[u8]> {
        Some(&self.prg_ram)
    }

    fn set_prg_ram(&mut self, data: &[u8]) {
        let len = data.len().min(self.prg_ram.len());
        self.prg_ram[..len].copy_from_slice(&data[..len]);
    }
}

/// `UxROM` (Mapper 2): simple 16K PRG bank switching.
///
/// One of the most common NES mappers, used by Mega Man, Castlevania,
/// Contra, and `DuckTales`.
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

/// `AxROM` (Mapper 7): 32K PRG bank switching with single-screen mirroring.
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
        let bank_1k = if chr_mode {
            // Mode 1: 1K,1K,1K,1K,2K,2K (inverted)
            match addr_usize >> 10 {
                0 => self.registers[2] as usize,          // R2
                1 => self.registers[3] as usize,          // R3
                2 => self.registers[4] as usize,          // R4
                3 => self.registers[5] as usize,          // R5
                4 => (self.registers[0] & 0xFE) as usize, // R0 (2K-aligned)
                5 => (self.registers[0] | 1) as usize,    // R0+1
                6 => (self.registers[1] & 0xFE) as usize, // R1 (2K-aligned)
                7 => (self.registers[1] | 1) as usize,    // R1+1
                _ => unreachable!(),
            }
        } else {
            // Mode 0: 2K,2K,1K,1K,1K,1K
            match addr_usize >> 10 {
                0 => (self.registers[0] & 0xFE) as usize, // R0 (2K-aligned)
                1 => (self.registers[0] | 1) as usize,    // R0+1
                2 => (self.registers[1] & 0xFE) as usize, // R1 (2K-aligned)
                3 => (self.registers[1] | 1) as usize,    // R1+1
                4 => self.registers[2] as usize,          // R2
                5 => self.registers[3] as usize,          // R3
                6 => self.registers[4] as usize,          // R4
                7 => self.registers[5] as usize,          // R5
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

        let bank_1k = if chr_mode {
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
        } else {
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

    fn prg_ram(&self) -> Option<&[u8]> {
        Some(&self.prg_ram)
    }

    fn set_prg_ram(&mut self, data: &[u8]) {
        let len = data.len().min(self.prg_ram.len());
        self.prg_ram[..len].copy_from_slice(&data[..len]);
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

/// `GxROM` (Mapper 66): Simple PRG + CHR bank switching.
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
        if self.horizontal_mirror {
            Mirroring::Horizontal
        } else {
            Mirroring::Vertical
        }
    }
}

/// `BxROM` (Mapper 34): Simple 32K PRG bank switching.
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
        Self {
            prg_rom,
            chr_ram: [0; 8192],
            mirroring,
            prg_bank: 0,
        }
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
        Self {
            prg_rom,
            chr_ram: [0; 8192],
            mirroring,
            prg_bank: 0,
        }
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

/// Mapper 87: simple CHR bank swap via $6000-$7FFF.
///
/// Used by ~10 games (City Connection, Ninja Jajamaru-kun). Writes to
/// $6000-$7FFF select an 8K CHR bank. Bits are swapped: written bit 0
/// drives CHR A15, written bit 1 drives CHR A14.
///
/// - PRG: 16K or 32K, no banking
/// - CHR: 8K switchable ROM
/// - Mirroring: fixed from header
struct Mapper87 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: Mirroring,
    chr_bank: u8,
}

impl Mapper87 {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self {
            prg_rom,
            chr_rom,
            mirroring,
            chr_bank: 0,
        }
    }
}

impl Mapper for Mapper87 {
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
        if (0x6000..=0x7FFF).contains(&addr) {
            // Bit swap: written bit 1 → CHR A14, written bit 0 → CHR A15
            self.chr_bank = ((value & 1) << 1) | ((value >> 1) & 1);
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        let bank_offset = self.chr_bank as usize * 8192;
        let index = (bank_offset + (addr as usize & 0x1FFF)) % self.chr_rom.len().max(1);
        self.chr_rom.get(index).copied().unwrap_or(0)
    }

    fn chr_write(&mut self, _addr: u16, _value: u8) {
        // CHR ROM — no writes
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

/// Mapper 206 (Namco 118 / MIMIC-1): simplified MMC3.
///
/// Used by ~30-50 games (Mappy-Land, Dragon Spirit, Gauntlet).
/// Same banking register interface as MMC3 ($8000/$8001) but:
/// - No scanline IRQ counter
/// - No PRG RAM at $6000-$7FFF
/// - No mirroring control ($A000 ignored) — fixed from iNES header
/// - CHR: 2×2K + 4×1K banks via R0-R5
/// - PRG: 2 switchable 8K banks via R6-R7, last two fixed
struct Mapper206 {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    mirroring: Mirroring,
    bank_select: u8,
    registers: [u8; 8],
}

impl Mapper206 {
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
            bank_select: 0,
            registers: [0; 8],
        }
    }

    fn prg_8k_count(&self) -> usize {
        self.prg_rom.len() / 8192
    }

    fn read_prg_8k(&self, bank: usize, offset: usize) -> u8 {
        let bank = bank % self.prg_8k_count();
        self.prg_rom[bank * 8192 + offset]
    }
}

impl Mapper for Mapper206 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0x9FFF => {
                let offset = (addr - 0x8000) as usize;
                self.read_prg_8k(self.registers[6] as usize & 0x0F, offset)
            }
            0xA000..=0xBFFF => {
                let offset = (addr - 0xA000) as usize;
                self.read_prg_8k(self.registers[7] as usize & 0x0F, offset)
            }
            0xC000..=0xDFFF => {
                let offset = (addr - 0xC000) as usize;
                self.read_prg_8k(self.prg_8k_count().saturating_sub(2), offset)
            }
            0xE000..=0xFFFF => {
                let offset = (addr - 0xE000) as usize;
                self.read_prg_8k(self.prg_8k_count().saturating_sub(1), offset)
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        if let 0x8000..=0x9FFF = addr {
            if addr & 1 == 0 {
                self.bank_select = value;
            } else {
                let reg = (self.bank_select & 0x07) as usize;
                self.registers[reg] = value;
            }
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        let addr_usize = (addr & 0x1FFF) as usize;
        let chr_mode = self.bank_select & 0x80 != 0;

        let bank_1k = if chr_mode {
            // Mode 1: 1K,1K,1K,1K,2K,2K (inverted)
            match addr_usize >> 10 {
                0 => self.registers[2] as usize & 0x3F,
                1 => self.registers[3] as usize & 0x3F,
                2 => self.registers[4] as usize & 0x3F,
                3 => self.registers[5] as usize & 0x3F,
                4 => (self.registers[0] & 0x3E) as usize,
                5 => (self.registers[0] | 1) as usize & 0x3F,
                6 => (self.registers[1] & 0x3E) as usize,
                7 => (self.registers[1] | 1) as usize & 0x3F,
                _ => unreachable!(),
            }
        } else {
            // Mode 0: 2K,2K,1K,1K,1K,1K
            match addr_usize >> 10 {
                0 => (self.registers[0] & 0x3E) as usize,
                1 => (self.registers[0] | 1) as usize & 0x3F,
                2 => (self.registers[1] & 0x3E) as usize,
                3 => (self.registers[1] | 1) as usize & 0x3F,
                4 => self.registers[2] as usize & 0x3F,
                5 => self.registers[3] as usize & 0x3F,
                6 => self.registers[4] as usize & 0x3F,
                7 => self.registers[5] as usize & 0x3F,
                _ => unreachable!(),
            }
        };

        let offset = addr_usize & 0x3FF;
        let index = (bank_1k * 1024 + offset) % self.chr.len();
        self.chr[index]
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        if self.chr_is_ram {
            let addr_usize = (addr & 0x1FFF) as usize;
            // For CHR RAM, write directly (banking not typically used with RAM)
            self.chr[addr_usize] = value;
        }
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

/// NINA-003/006 (Mapper 79, AVE): PRG and CHR switching via $4100-$5FFF.
///
/// Used by unlicensed AVE games (Krazy Kreatures, Tiles of Fate, Deathbots).
///
/// - PRG: 32K switchable at $8000-$FFFF (bits 3 of register)
/// - CHR: 8K switchable (bits 0-2 of register)
/// - Mirroring: fixed from header
/// - Register: bits 0-2 = CHR bank, bit 3 = PRG bank; at $4100-$5FFF
struct Nina003 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: Mirroring,
    prg_bank: u8,
    chr_bank: u8,
}

impl Nina003 {
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

impl Mapper for Nina003 {
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
        if (0x4100..=0x5FFF).contains(&addr) {
            self.chr_bank = value & 0x07;
            self.prg_bank = (value >> 3) & 0x01;
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

/// Taito TC0190 (Mapper 33): PRG and CHR bank switching.
///
/// Used by Don Doko Don, Akira, Insector X, Power Blazer.
///
/// - PRG: Two 8K switchable banks at $8000 and $A000; last two 8K fixed
/// - CHR: Two 2K banks + four 1K banks
/// - Mirroring: switchable via bit 6 of $8000 register
struct TaitoTc0190 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: Mirroring,
    prg_bank_0: u8,
    prg_bank_1: u8,
    chr_banks: [u8; 6],
}

impl TaitoTc0190 {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self {
            prg_rom,
            chr_rom,
            mirroring,
            prg_bank_0: 0,
            prg_bank_1: 0,
            chr_banks: [0; 6],
        }
    }

    fn prg_8k_count(&self) -> usize {
        self.prg_rom.len() / 8192
    }

    fn read_prg_8k(&self, bank: usize, offset: usize) -> u8 {
        let bank = bank % self.prg_8k_count();
        self.prg_rom[bank * 8192 + offset]
    }

    fn read_chr(&self, bank: usize, offset: usize, bank_size: usize) -> u8 {
        if self.chr_rom.is_empty() {
            return 0;
        }
        let total_banks = self.chr_rom.len() / bank_size;
        let bank = bank % total_banks.max(1);
        let index = bank * bank_size + offset;
        self.chr_rom.get(index).copied().unwrap_or(0)
    }
}

impl Mapper for TaitoTc0190 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0x9FFF => {
                self.read_prg_8k(self.prg_bank_0 as usize & 0x3F, (addr - 0x8000) as usize)
            }
            0xA000..=0xBFFF => {
                self.read_prg_8k(self.prg_bank_1 as usize & 0x3F, (addr - 0xA000) as usize)
            }
            0xC000..=0xDFFF => {
                self.read_prg_8k(self.prg_8k_count() - 2, (addr - 0xC000) as usize)
            }
            0xE000..=0xFFFF => {
                self.read_prg_8k(self.prg_8k_count() - 1, (addr - 0xE000) as usize)
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x8000 => {
                self.prg_bank_0 = value & 0x3F;
                self.mirroring = if value & 0x40 != 0 {
                    Mirroring::Horizontal
                } else {
                    Mirroring::Vertical
                };
            }
            0x8001 => self.prg_bank_1 = value & 0x3F,
            0x8002 => self.chr_banks[0] = value,    // 2K at $0000
            0x8003 => self.chr_banks[1] = value,    // 2K at $0800
            0xA000 => self.chr_banks[2] = value,    // 1K at $1000
            0xA001 => self.chr_banks[3] = value,    // 1K at $1400
            0xA002 => self.chr_banks[4] = value,    // 1K at $1800
            0xA003 => self.chr_banks[5] = value,    // 1K at $1C00
            _ => {}
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        let addr_usize = addr as usize & 0x1FFF;
        match addr_usize {
            0x0000..=0x07FF => self.read_chr(self.chr_banks[0] as usize, addr_usize & 0x7FF, 2048),
            0x0800..=0x0FFF => self.read_chr(self.chr_banks[1] as usize, addr_usize & 0x7FF, 2048),
            0x1000..=0x13FF => self.read_chr(self.chr_banks[2] as usize, addr_usize & 0x3FF, 1024),
            0x1400..=0x17FF => self.read_chr(self.chr_banks[3] as usize, addr_usize & 0x3FF, 1024),
            0x1800..=0x1BFF => self.read_chr(self.chr_banks[4] as usize, addr_usize & 0x3FF, 1024),
            0x1C00..=0x1FFF => self.read_chr(self.chr_banks[5] as usize, addr_usize & 0x3FF, 1024),
            _ => 0,
        }
    }

    fn chr_write(&mut self, _addr: u16, _value: u8) {}

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

/// Irem G-101 (Mapper 32): 8K PRG bank switching with mode select.
///
/// Used by Image Fight, Major League, Kaiketsu Yanchamaru 2, Ai Senshi Nicol.
///
/// - PRG: Two switchable 8K banks + two fixed (last two); mode bit swaps layout
/// - CHR: Eight 1K switchable banks
/// - Mirroring: switchable H/V via bit 0 of $9000
struct IremG101 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    chr_is_ram: bool,
    mirroring: Mirroring,
    prg_bank_0: u8,
    prg_bank_1: u8,
    prg_mode: bool,
    chr_banks: [u8; 8],
}

impl IremG101 {
    fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>, mirroring: Mirroring) -> Self {
        let chr_is_ram = chr_data.is_empty();
        let chr_rom = if chr_is_ram {
            vec![0u8; 8192]
        } else {
            chr_data
        };
        Self {
            prg_rom,
            chr_rom,
            chr_is_ram,
            mirroring,
            prg_bank_0: 0,
            prg_bank_1: 0,
            prg_mode: false,
            chr_banks: [0; 8],
        }
    }

    fn prg_8k_count(&self) -> usize {
        self.prg_rom.len() / 8192
    }

    fn read_prg_8k(&self, bank: usize, offset: usize) -> u8 {
        let bank = bank % self.prg_8k_count();
        self.prg_rom[bank * 8192 + offset]
    }
}

impl Mapper for IremG101 {
    fn cpu_read(&self, addr: u16) -> u8 {
        let last = self.prg_8k_count();
        match addr {
            0x8000..=0x9FFF => {
                let bank = if self.prg_mode {
                    last - 2
                } else {
                    self.prg_bank_0 as usize
                };
                self.read_prg_8k(bank, (addr - 0x8000) as usize)
            }
            0xA000..=0xBFFF => {
                self.read_prg_8k(self.prg_bank_1 as usize, (addr - 0xA000) as usize)
            }
            0xC000..=0xDFFF => {
                let bank = if self.prg_mode {
                    self.prg_bank_0 as usize
                } else {
                    last - 2
                };
                self.read_prg_8k(bank, (addr - 0xC000) as usize)
            }
            0xE000..=0xFFFF => {
                self.read_prg_8k(last - 1, (addr - 0xE000) as usize)
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x8000..=0x8FFF => self.prg_bank_0 = value & 0x1F,
            0x9000..=0x9FFF => {
                self.mirroring = if value & 0x01 != 0 {
                    Mirroring::Horizontal
                } else {
                    Mirroring::Vertical
                };
                self.prg_mode = value & 0x02 != 0;
            }
            0xA000..=0xAFFF => self.prg_bank_1 = value & 0x1F,
            0xB000..=0xBFFF => {
                let reg = (addr & 0x07) as usize;
                if reg < 8 {
                    self.chr_banks[reg] = value;
                }
            }
            _ => {}
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        let addr_usize = addr as usize & 0x1FFF;
        let bank_index = addr_usize / 1024;
        let bank = self.chr_banks[bank_index] as usize;
        let offset = addr_usize & 0x3FF;
        let index = (bank * 1024 + offset) % self.chr_rom.len();
        self.chr_rom[index]
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        if self.chr_is_ram {
            self.chr_rom[(addr as usize) & 0x1FFF] = value;
        }
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

/// Irem H3001 (Mapper 65): 8K PRG switching with IRQ timer.
///
/// Used by Spartan X 2, Daiku no Gen San 2.
///
/// - PRG: Three switchable 8K banks, last 8K fixed
/// - CHR: Eight 1K switchable banks
/// - Mirroring: switchable H/V
/// - IRQ: 16-bit countdown timer
struct IremH3001 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: Mirroring,
    prg_banks: [u8; 3],
    chr_banks: [u8; 8],
    irq_enabled: bool,
    irq_counter: u16,
    irq_latch: u16,
    irq_pending: bool,
}

impl IremH3001 {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self {
            prg_rom,
            chr_rom,
            mirroring,
            prg_banks: [0; 3],
            chr_banks: [0; 8],
            irq_enabled: false,
            irq_counter: 0,
            irq_latch: 0,
            irq_pending: false,
        }
    }

    fn prg_8k_count(&self) -> usize {
        self.prg_rom.len() / 8192
    }

    fn read_prg_8k(&self, bank: usize, offset: usize) -> u8 {
        let bank = bank % self.prg_8k_count();
        self.prg_rom[bank * 8192 + offset]
    }
}

impl Mapper for IremH3001 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0x9FFF => {
                self.read_prg_8k(self.prg_banks[0] as usize, (addr - 0x8000) as usize)
            }
            0xA000..=0xBFFF => {
                self.read_prg_8k(self.prg_banks[1] as usize, (addr - 0xA000) as usize)
            }
            0xC000..=0xDFFF => {
                self.read_prg_8k(self.prg_banks[2] as usize, (addr - 0xC000) as usize)
            }
            0xE000..=0xFFFF => {
                self.read_prg_8k(self.prg_8k_count() - 1, (addr - 0xE000) as usize)
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x8000 => self.prg_banks[0] = value,
            0xA000 => self.prg_banks[1] = value,
            0xC000 => self.prg_banks[2] = value,
            0x9001 => {
                self.mirroring = if value & 0x80 != 0 {
                    Mirroring::Horizontal
                } else {
                    Mirroring::Vertical
                };
            }
            0x9003 => {
                self.irq_enabled = value & 0x80 != 0;
                self.irq_pending = false;
            }
            0x9004 => {
                self.irq_counter = self.irq_latch;
                self.irq_pending = false;
            }
            0x9005 => {
                self.irq_latch = (self.irq_latch & 0x00FF) | (u16::from(value) << 8);
            }
            0x9006 => {
                self.irq_latch = (self.irq_latch & 0xFF00) | u16::from(value);
            }
            0xB000..=0xB007 => {
                self.chr_banks[(addr - 0xB000) as usize] = value;
            }
            _ => {}
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        if self.chr_rom.is_empty() {
            return 0;
        }
        let addr_usize = addr as usize & 0x1FFF;
        let bank_index = addr_usize / 1024;
        let bank = self.chr_banks[bank_index] as usize;
        let offset = addr_usize & 0x3FF;
        let index = (bank * 1024 + offset) % self.chr_rom.len();
        self.chr_rom[index]
    }

    fn chr_write(&mut self, _addr: u16, _value: u8) {}

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }
}

/// Konami VRC2/VRC4 (Mappers 21, 22, 23, 25): fine-grained PRG/CHR switching.
///
/// Used by Gradius II, Crisis Force, Parodius Da!, Tiny Toon Adventures (JP),
/// Bio Miracle Bokutte Upa, Ganbare Goemon Gaiden.
///
/// - PRG: Two switchable 8K banks, mode bit swaps $8000/$C000
/// - CHR: Eight 1K banks (each set by two 4-bit registers, low/high nibble)
/// - Mirroring: switchable
/// - IRQ: scanline counter (VRC4 only, but present in all variants)
///
/// Address line wiring varies by submapper:
///   Mapper 21: A1/A2 (VRC4a/VRC4c)
///   Mapper 22: A0/A1 (VRC2a)
///   Mapper 23: A0/A1 (VRC2b/VRC4e)
///   Mapper 25: A0/A1 swapped (VRC4b/VRC4d)
struct Vrc2Vrc4 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    chr_is_ram: bool,
    mirroring: Mirroring,
    prg_bank_0: u8,
    prg_bank_1: u8,
    prg_mode: bool,
    chr_banks_lo: [u8; 8],
    chr_banks_hi: [u8; 8],
    /// Address bit mapping: (low_bit_shift, high_bit_shift) applied to address
    /// before extracting register index.
    addr_shift_lo: u8,
    addr_shift_hi: u8,
    /// VRC2 mode: CHR granularity is halved (address >> 1)
    vrc2_mode: bool,
    irq_latch: u8,
    irq_counter: u8,
    irq_prescaler: i16,
    irq_enabled: bool,
    irq_enabled_after_ack: bool,
    irq_mode_cycle: bool,
    irq_pending: bool,
}

impl Vrc2Vrc4 {
    fn new(
        prg_rom: Vec<u8>,
        chr_data: Vec<u8>,
        mirroring: Mirroring,
        addr_shift_lo: u8,
        addr_shift_hi: u8,
        vrc2_mode: bool,
    ) -> Self {
        let chr_is_ram = chr_data.is_empty();
        let chr_rom = if chr_is_ram {
            vec![0u8; 8192]
        } else {
            chr_data
        };
        Self {
            prg_rom,
            chr_rom,
            chr_is_ram,
            mirroring,
            prg_bank_0: 0,
            prg_bank_1: 0,
            prg_mode: false,
            chr_banks_lo: [0; 8],
            chr_banks_hi: [0; 8],
            addr_shift_lo,
            addr_shift_hi,
            vrc2_mode,
            irq_latch: 0,
            irq_counter: 0,
            irq_prescaler: 341,
            irq_enabled: false,
            irq_enabled_after_ack: false,
            irq_mode_cycle: false,
            irq_pending: false,
        }
    }

    fn prg_8k_count(&self) -> usize {
        self.prg_rom.len() / 8192
    }

    fn read_prg_8k(&self, bank: usize, offset: usize) -> u8 {
        let bank = bank % self.prg_8k_count();
        self.prg_rom[bank * 8192 + offset]
    }

    /// Remap an address to extract the two relevant address bits used by this
    /// VRC variant to distinguish sub-registers.
    fn remap_addr(&self, addr: u16) -> u16 {
        let base = addr & 0xF000;
        let bit0 = (addr >> self.addr_shift_lo) & 1;
        let bit1 = (addr >> self.addr_shift_hi) & 1;
        base | (bit1 << 1) | bit0
    }

    fn chr_bank_value(&self, index: usize) -> usize {
        let lo = self.chr_banks_lo[index] as usize & 0x0F;
        let hi = self.chr_banks_hi[index] as usize & 0x1F;
        let bank = (hi << 4) | lo;
        if self.vrc2_mode { bank >> 1 } else { bank }
    }
}

impl Mapper for Vrc2Vrc4 {
    fn cpu_read(&self, addr: u16) -> u8 {
        let last = self.prg_8k_count();
        match addr {
            0x8000..=0x9FFF => {
                let bank = if self.prg_mode {
                    last - 2
                } else {
                    self.prg_bank_0 as usize & 0x1F
                };
                self.read_prg_8k(bank, (addr - 0x8000) as usize)
            }
            0xA000..=0xBFFF => {
                self.read_prg_8k(self.prg_bank_1 as usize & 0x1F, (addr - 0xA000) as usize)
            }
            0xC000..=0xDFFF => {
                let bank = if self.prg_mode {
                    self.prg_bank_0 as usize & 0x1F
                } else {
                    last - 2
                };
                self.read_prg_8k(bank, (addr - 0xC000) as usize)
            }
            0xE000..=0xFFFF => {
                self.read_prg_8k(last - 1, (addr - 0xE000) as usize)
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        let mapped = self.remap_addr(addr);
        match mapped {
            0x8000..=0x8003 => self.prg_bank_0 = value & 0x1F,
            0x9000 | 0x9002 => {
                self.mirroring = match value & 0x03 {
                    0 => Mirroring::Vertical,
                    1 => Mirroring::Horizontal,
                    2 => Mirroring::SingleScreenLower,
                    3 => Mirroring::SingleScreenUpper,
                    _ => unreachable!(),
                };
            }
            0x9001 | 0x9003 => {
                self.prg_mode = value & 0x02 != 0;
            }
            0xA000..=0xA003 => self.prg_bank_1 = value & 0x1F,
            0xB000 => self.chr_banks_lo[0] = value,
            0xB001 => self.chr_banks_hi[0] = value,
            0xB002 => self.chr_banks_lo[1] = value,
            0xB003 => self.chr_banks_hi[1] = value,
            0xC000 => self.chr_banks_lo[2] = value,
            0xC001 => self.chr_banks_hi[2] = value,
            0xC002 => self.chr_banks_lo[3] = value,
            0xC003 => self.chr_banks_hi[3] = value,
            0xD000 => self.chr_banks_lo[4] = value,
            0xD001 => self.chr_banks_hi[4] = value,
            0xD002 => self.chr_banks_lo[5] = value,
            0xD003 => self.chr_banks_hi[5] = value,
            0xE000 => self.chr_banks_lo[6] = value,
            0xE001 => self.chr_banks_hi[6] = value,
            0xE002 => self.chr_banks_lo[7] = value,
            0xE003 => self.chr_banks_hi[7] = value,
            0xF000 => {
                self.irq_latch = (self.irq_latch & 0xF0) | (value & 0x0F);
            }
            0xF001 => {
                self.irq_latch = (self.irq_latch & 0x0F) | (value << 4);
            }
            0xF002 => {
                self.irq_pending = false;
                self.irq_enabled_after_ack = value & 0x01 != 0;
                self.irq_enabled = value & 0x02 != 0;
                self.irq_mode_cycle = value & 0x04 != 0;
                if self.irq_enabled {
                    self.irq_counter = self.irq_latch;
                    self.irq_prescaler = 341;
                }
            }
            0xF003 => {
                self.irq_pending = false;
                self.irq_enabled = self.irq_enabled_after_ack;
            }
            _ => {}
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        let addr_usize = addr as usize & 0x1FFF;
        let bank_index = addr_usize / 1024;
        let bank = self.chr_bank_value(bank_index);
        let offset = addr_usize & 0x3FF;
        let index = (bank * 1024 + offset) % self.chr_rom.len();
        self.chr_rom[index]
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        if self.chr_is_ram {
            self.chr_rom[(addr as usize) & 0x1FFF] = value;
        }
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }
}

/// Sunsoft 5B expansion audio channel (AY-3-8910 style square wave).
struct Sunsoft5bChannel {
    period: u16,
    volume: u8,
    counter: u16,
    output: bool,
    tone_disable: bool,
}

impl Sunsoft5bChannel {
    fn new() -> Self {
        Self {
            period: 0,
            volume: 0,
            counter: 0,
            output: false,
            tone_disable: false,
        }
    }

    fn tick(&mut self) {
        if self.counter == 0 {
            self.counter = self.period;
            self.output = !self.output;
        } else {
            self.counter -= 1;
        }
    }

    fn sample(&self) -> f32 {
        if self.tone_disable || self.output {
            f32::from(self.volume) / 15.0
        } else {
            0.0
        }
    }
}

/// Sunsoft FME-7 (Mapper 69): versatile bank switching with IRQ timer.
///
/// Used by Gimmick!, Batman: Return of the Joker, Hebereke (Ufouria),
/// Gremlins 2 (JP), Barcode World.
///
/// - PRG: Four 8K windows (three switchable + last fixed, or all four switchable)
/// - CHR: Eight 1K switchable banks
/// - PRG RAM: 8K at $6000-$7FFF (optional, bank-selectable)
/// - Mirroring: switchable
/// - IRQ: 16-bit countdown timer
/// - Expansion audio: Sunsoft 5B — 3 square-wave channels (AY-3-8910 subset)
struct SunsoftFme7 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    chr_is_ram: bool,
    prg_ram: [u8; 8192],
    mirroring: Mirroring,
    command: u8,
    chr_banks: [u8; 8],
    prg_banks: [u8; 4],
    prg_ram_enabled: bool,
    prg_ram_selected: bool,
    irq_enabled: bool,
    irq_counter_enabled: bool,
    irq_counter: u16,
    irq_pending: bool,
    // Sunsoft 5B expansion audio
    audio_channels: [Sunsoft5bChannel; 3],
    audio_command: u8,
    /// Divider: 5B audio clocks at CPU/16.
    audio_divider: u8,
}

impl SunsoftFme7 {
    fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>, mirroring: Mirroring) -> Self {
        let chr_is_ram = chr_data.is_empty();
        let chr_rom = if chr_is_ram {
            vec![0u8; 8192]
        } else {
            chr_data
        };
        let prg_8k_count = prg_rom.len() / 8192;
        Self {
            prg_rom,
            chr_rom,
            chr_is_ram,
            prg_ram: [0; 8192],
            mirroring,
            command: 0,
            chr_banks: [0; 8],
            prg_banks: [0, 0, 0, (prg_8k_count.saturating_sub(1)) as u8],
            prg_ram_enabled: false,
            prg_ram_selected: false,
            irq_enabled: false,
            irq_counter_enabled: false,
            irq_counter: 0,
            irq_pending: false,
            audio_channels: [
                Sunsoft5bChannel::new(),
                Sunsoft5bChannel::new(),
                Sunsoft5bChannel::new(),
            ],
            audio_command: 0,
            audio_divider: 0,
        }
    }

    fn prg_8k_count(&self) -> usize {
        self.prg_rom.len() / 8192
    }

    fn read_prg_8k(&self, bank: usize, offset: usize) -> u8 {
        let bank = bank % self.prg_8k_count();
        self.prg_rom[bank * 8192 + offset]
    }

    /// Write to a Sunsoft 5B audio register (AY-3-8910 subset).
    ///
    /// Registers 0-5: channel period (low/high pairs for channels A/B/C).
    /// Register 7: tone enable (bits 0-2, active-low).
    /// Registers 8-10: channel volume (bits 0-3).
    fn write_audio_register(&mut self, reg: u8, value: u8) {
        match reg {
            0 => {
                self.audio_channels[0].period =
                    (self.audio_channels[0].period & 0xFF00) | u16::from(value);
            }
            1 => {
                self.audio_channels[0].period =
                    (self.audio_channels[0].period & 0x00FF) | (u16::from(value & 0x0F) << 8);
            }
            2 => {
                self.audio_channels[1].period =
                    (self.audio_channels[1].period & 0xFF00) | u16::from(value);
            }
            3 => {
                self.audio_channels[1].period =
                    (self.audio_channels[1].period & 0x00FF) | (u16::from(value & 0x0F) << 8);
            }
            4 => {
                self.audio_channels[2].period =
                    (self.audio_channels[2].period & 0xFF00) | u16::from(value);
            }
            5 => {
                self.audio_channels[2].period =
                    (self.audio_channels[2].period & 0x00FF) | (u16::from(value & 0x0F) << 8);
            }
            7 => {
                // Bits 0-2: tone disable (active-low per channel)
                self.audio_channels[0].tone_disable = value & 0x01 != 0;
                self.audio_channels[1].tone_disable = value & 0x02 != 0;
                self.audio_channels[2].tone_disable = value & 0x04 != 0;
            }
            8 => self.audio_channels[0].volume = value & 0x0F,
            9 => self.audio_channels[1].volume = value & 0x0F,
            10 => self.audio_channels[2].volume = value & 0x0F,
            _ => {} // Noise, envelope, and I/O registers not used by NES games
        }
    }
}

impl Mapper for SunsoftFme7 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                if self.prg_ram_selected {
                    if self.prg_ram_enabled {
                        self.prg_ram[(addr - 0x6000) as usize]
                    } else {
                        0
                    }
                } else {
                    // ROM bank mapped to $6000
                    let bank = (self.prg_banks[0] as usize & 0x3F) % self.prg_8k_count();
                    self.prg_rom[bank * 8192 + (addr - 0x6000) as usize]
                }
            }
            0x8000..=0x9FFF => {
                self.read_prg_8k(self.prg_banks[1] as usize & 0x3F, (addr - 0x8000) as usize)
            }
            0xA000..=0xBFFF => {
                self.read_prg_8k(self.prg_banks[2] as usize & 0x3F, (addr - 0xA000) as usize)
            }
            0xC000..=0xDFFF => {
                self.read_prg_8k(self.prg_banks[3] as usize & 0x3F, (addr - 0xC000) as usize)
            }
            0xE000..=0xFFFF => {
                self.read_prg_8k(self.prg_8k_count() - 1, (addr - 0xE000) as usize)
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                if self.prg_ram_selected && self.prg_ram_enabled {
                    self.prg_ram[(addr - 0x6000) as usize] = value;
                }
            }
            0x8000..=0x9FFF => {
                self.command = value & 0x0F;
            }
            0xA000..=0xBFFF => {
                match self.command {
                    0..=7 => self.chr_banks[self.command as usize] = value,
                    8 => {
                        self.prg_ram_selected = value & 0x40 != 0;
                        self.prg_ram_enabled = value & 0x80 != 0;
                        self.prg_banks[0] = value & 0x3F;
                    }
                    9 => self.prg_banks[1] = value & 0x3F,
                    10 => self.prg_banks[2] = value & 0x3F,
                    11 => self.prg_banks[3] = value & 0x3F,
                    12 => {
                        self.mirroring = match value & 0x03 {
                            0 => Mirroring::Vertical,
                            1 => Mirroring::Horizontal,
                            2 => Mirroring::SingleScreenLower,
                            3 => Mirroring::SingleScreenUpper,
                            _ => unreachable!(),
                        };
                    }
                    13 => {
                        self.irq_counter_enabled = value & 0x80 != 0;
                        self.irq_enabled = value & 0x01 != 0;
                        self.irq_pending = false;
                    }
                    14 => {
                        self.irq_counter = (self.irq_counter & 0xFF00) | u16::from(value);
                    }
                    15 => {
                        self.irq_counter = (self.irq_counter & 0x00FF) | (u16::from(value) << 8);
                    }
                    _ => {}
                }
            }
            // Sunsoft 5B expansion audio ports
            0xC000..=0xDFFF => {
                self.audio_command = value & 0x0F;
            }
            0xE000..=0xFFFF => {
                self.write_audio_register(self.audio_command, value);
            }
            _ => {}
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        let addr_usize = addr as usize & 0x1FFF;
        let bank_index = addr_usize / 1024;
        let bank = self.chr_banks[bank_index] as usize;
        let offset = addr_usize & 0x3FF;
        let index = (bank * 1024 + offset) % self.chr_rom.len();
        self.chr_rom[index]
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        if self.chr_is_ram {
            self.chr_rom[(addr as usize) & 0x1FFF] = value;
        }
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn tick_audio(&mut self) {
        // 5B audio divider: clocks at CPU/16 (~111.8 kHz NTSC)
        self.audio_divider = self.audio_divider.wrapping_add(1);
        if self.audio_divider & 0x0F == 0 {
            for ch in &mut self.audio_channels {
                ch.tick();
            }
        }
    }

    fn audio_output(&self) -> f32 {
        let sum = self.audio_channels[0].sample()
            + self.audio_channels[1].sample()
            + self.audio_channels[2].sample();
        // Scale to ~0.15 total to balance with APU output (~0.5 peak)
        sum * 0.05
    }

    fn prg_ram(&self) -> Option<&[u8]> {
        if self.prg_ram_selected {
            Some(&self.prg_ram)
        } else {
            None
        }
    }

    fn set_prg_ram(&mut self, data: &[u8]) {
        let len = data.len().min(self.prg_ram.len());
        self.prg_ram[..len].copy_from_slice(&data[..len]);
    }
}

/// Bandai FCG / LZ93D50 (Mapper 16, 159): 16K PRG + 1K CHR + IRQ counter.
///
/// Used by Dragon Ball Z series, SD Gundam Gaiden, Famicom Jump.
///
/// - PRG: 16K switchable at $8000-$BFFF, last 16K fixed
/// - CHR: Eight 1K switchable banks
/// - Mirroring: switchable
/// - IRQ: 16-bit countdown timer (decrements each CPU cycle)
///
/// Mapper 159 is functionally identical (128-byte EEPROM variant; EEPROM
/// not emulated). Both use the LZ93D50 register layout at $8000-$800D.
struct BandaiFcg {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: Mirroring,
    prg_bank: u8,
    chr_banks: [u8; 8],
    irq_enabled: bool,
    irq_counter: u16,
    irq_latch: u16,
    irq_pending: bool,
}

impl BandaiFcg {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self {
            prg_rom,
            chr_rom,
            mirroring,
            prg_bank: 0,
            chr_banks: [0; 8],
            irq_enabled: false,
            irq_counter: 0,
            irq_latch: 0,
            irq_pending: false,
        }
    }

    fn prg_16k_count(&self) -> usize {
        self.prg_rom.len() / 16384
    }

    fn write_register(&mut self, reg: u16, value: u8) {
        let reg = reg & 0x000F;
        match reg {
            0x0..=0x7 => {
                self.chr_banks[reg as usize] = value;
            }
            0x8 => self.prg_bank = value & 0x0F,
            0x9 => {
                self.mirroring = match value & 0x03 {
                    0 => Mirroring::Vertical,
                    1 => Mirroring::Horizontal,
                    2 => Mirroring::SingleScreenLower,
                    3 => Mirroring::SingleScreenUpper,
                    _ => unreachable!(),
                };
            }
            0xA => {
                self.irq_enabled = value & 0x01 != 0;
                self.irq_counter = self.irq_latch;
                self.irq_pending = false;
            }
            0xB => {
                self.irq_latch = (self.irq_latch & 0xFF00) | u16::from(value);
            }
            0xC => {
                self.irq_latch = (self.irq_latch & 0x00FF) | (u16::from(value) << 8);
            }
            _ => {} // $D = EEPROM, not emulated
        }
    }
}

impl Mapper for BandaiFcg {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xBFFF => {
                let bank = self.prg_bank as usize % self.prg_16k_count();
                self.prg_rom[bank * 16384 + (addr - 0x8000) as usize]
            }
            0xC000..=0xFFFF => {
                let last = self.prg_16k_count().saturating_sub(1);
                self.prg_rom[last * 16384 + (addr - 0xC000) as usize]
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        // LZ93D50: registers at $8000-$800D
        if (0x6000..=0xFFFF).contains(&addr) {
            self.write_register(addr, value);
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        if self.chr_rom.is_empty() {
            return 0;
        }
        let addr_usize = addr as usize & 0x1FFF;
        let bank_index = addr_usize / 1024;
        let bank = self.chr_banks[bank_index] as usize;
        let offset = addr_usize & 0x3FF;
        let index = (bank * 1024 + offset) % self.chr_rom.len();
        self.chr_rom[index]
    }

    fn chr_write(&mut self, _addr: u16, _value: u8) {}

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }
}

/// Jaleco SS88006 (Mapper 18): 8K PRG + 1K CHR with configurable IRQ timer.
///
/// Used by Magic John (Totally Rad), Pizza Pop!, Ninja Jajamaru: Ginga
/// Daisakusen. All Japanese releases.
///
/// - PRG: Three switchable 8K banks + last 8K fixed
/// - CHR: Eight 1K banks (each set by low/high nibble register pair)
/// - Mirroring: switchable
/// - IRQ: 16-bit timer with selectable bit-width (4/8/12/16-bit)
struct JalecoSs88006 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    chr_is_ram: bool,
    prg_ram: [u8; 8192],
    prg_ram_enabled: bool,
    mirroring: Mirroring,
    prg_banks: [u8; 3],
    chr_banks_lo: [u8; 8],
    chr_banks_hi: [u8; 8],
    irq_counter: u16,
    irq_latch: u16,
    irq_enabled: bool,
    irq_pending: bool,
    /// IRQ counter bit-width mask: $000F, $00FF, $0FFF, or $FFFF.
    irq_mask: u16,
}

impl JalecoSs88006 {
    fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>, mirroring: Mirroring) -> Self {
        let chr_is_ram = chr_data.is_empty();
        let chr_rom = if chr_is_ram {
            vec![0u8; 8192]
        } else {
            chr_data
        };
        Self {
            prg_rom,
            chr_rom,
            chr_is_ram,
            prg_ram: [0; 8192],
            prg_ram_enabled: false,
            mirroring,
            prg_banks: [0; 3],
            chr_banks_lo: [0; 8],
            chr_banks_hi: [0; 8],
            irq_counter: 0,
            irq_latch: 0,
            irq_enabled: false,
            irq_pending: false,
            irq_mask: 0xFFFF,
        }
    }

    fn prg_8k_count(&self) -> usize {
        self.prg_rom.len() / 8192
    }

    fn read_prg_8k(&self, bank: usize, offset: usize) -> u8 {
        let bank = bank % self.prg_8k_count();
        self.prg_rom[bank * 8192 + offset]
    }

    fn chr_bank_value(&self, index: usize) -> usize {
        let lo = self.chr_banks_lo[index] as usize & 0x0F;
        let hi = self.chr_banks_hi[index] as usize & 0x0F;
        (hi << 4) | lo
    }
}

impl Mapper for JalecoSs88006 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                if self.prg_ram_enabled {
                    self.prg_ram[(addr - 0x6000) as usize]
                } else {
                    0
                }
            }
            0x8000..=0x9FFF => {
                self.read_prg_8k(self.prg_banks[0] as usize & 0x3F, (addr - 0x8000) as usize)
            }
            0xA000..=0xBFFF => {
                self.read_prg_8k(self.prg_banks[1] as usize & 0x3F, (addr - 0xA000) as usize)
            }
            0xC000..=0xDFFF => {
                self.read_prg_8k(self.prg_banks[2] as usize & 0x3F, (addr - 0xC000) as usize)
            }
            0xE000..=0xFFFF => {
                self.read_prg_8k(self.prg_8k_count() - 1, (addr - 0xE000) as usize)
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                if self.prg_ram_enabled {
                    self.prg_ram[(addr - 0x6000) as usize] = value;
                }
            }
            // PRG bank 0 low/high nibble
            0x8000 => self.prg_banks[0] = (self.prg_banks[0] & 0xF0) | (value & 0x0F),
            0x8001 => self.prg_banks[0] = (self.prg_banks[0] & 0x0F) | ((value & 0x03) << 4),
            // PRG bank 1
            0x8002 => self.prg_banks[1] = (self.prg_banks[1] & 0xF0) | (value & 0x0F),
            0x8003 => self.prg_banks[1] = (self.prg_banks[1] & 0x0F) | ((value & 0x03) << 4),
            // PRG bank 2
            0x9000 => self.prg_banks[2] = (self.prg_banks[2] & 0xF0) | (value & 0x0F),
            0x9001 => self.prg_banks[2] = (self.prg_banks[2] & 0x0F) | ((value & 0x03) << 4),
            // PRG RAM enable
            0x9002 => self.prg_ram_enabled = value & 0x01 != 0,
            // CHR banks (low/high nibble pairs)
            0xA000 => self.chr_banks_lo[0] = value,
            0xA001 => self.chr_banks_hi[0] = value,
            0xA002 => self.chr_banks_lo[1] = value,
            0xA003 => self.chr_banks_hi[1] = value,
            0xB000 => self.chr_banks_lo[2] = value,
            0xB001 => self.chr_banks_hi[2] = value,
            0xB002 => self.chr_banks_lo[3] = value,
            0xB003 => self.chr_banks_hi[3] = value,
            0xC000 => self.chr_banks_lo[4] = value,
            0xC001 => self.chr_banks_hi[4] = value,
            0xC002 => self.chr_banks_lo[5] = value,
            0xC003 => self.chr_banks_hi[5] = value,
            0xD000 => self.chr_banks_lo[6] = value,
            0xD001 => self.chr_banks_hi[6] = value,
            0xD002 => self.chr_banks_lo[7] = value,
            0xD003 => self.chr_banks_hi[7] = value,
            // IRQ reload nibbles
            0xE000 => self.irq_latch = (self.irq_latch & 0xFFF0) | u16::from(value & 0x0F),
            0xE001 => self.irq_latch = (self.irq_latch & 0xFF0F) | (u16::from(value & 0x0F) << 4),
            0xE002 => self.irq_latch = (self.irq_latch & 0xF0FF) | (u16::from(value & 0x0F) << 8),
            0xE003 => self.irq_latch = (self.irq_latch & 0x0FFF) | (u16::from(value & 0x0F) << 12),
            // IRQ acknowledge + reload
            0xF000 => {
                self.irq_pending = false;
                self.irq_counter = self.irq_latch;
            }
            // IRQ control
            0xF001 => {
                self.irq_pending = false;
                self.irq_enabled = value & 0x01 != 0;
                // Bit-width selection: bit 3 = 4-bit, bit 2 = 8-bit, bit 1 = 12-bit
                self.irq_mask = if value & 0x08 != 0 {
                    0x000F
                } else if value & 0x04 != 0 {
                    0x00FF
                } else if value & 0x02 != 0 {
                    0x0FFF
                } else {
                    0xFFFF
                };
            }
            // Mirroring
            0xF002 => {
                self.mirroring = match value & 0x03 {
                    0 => Mirroring::Horizontal,
                    1 => Mirroring::Vertical,
                    2 => Mirroring::SingleScreenLower,
                    3 => Mirroring::SingleScreenUpper,
                    _ => unreachable!(),
                };
            }
            _ => {}
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        let addr_usize = addr as usize & 0x1FFF;
        let bank_index = addr_usize / 1024;
        let bank = self.chr_bank_value(bank_index);
        let offset = addr_usize & 0x3FF;
        let index = (bank * 1024 + offset) % self.chr_rom.len();
        self.chr_rom[index]
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        if self.chr_is_ram {
            self.chr_rom[(addr as usize) & 0x1FFF] = value;
        }
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }
}

/// Namco 163 (Mapper 19): PRG/CHR banking with wavetable expansion audio.
///
/// Used by Megami Tensei II, Rolling Thunder, Star Wars (Namco), Final Lap,
/// King of Kings, Sangokushi I & II.
///
/// - PRG: Three switchable 8K banks + last 8K fixed
/// - CHR: Eight 1K pattern table banks + four nametable banks (CIRAM or ROM)
/// - IRQ: 15-bit up-counter, fires at $7FFF
/// - Audio: 1-8 wavetable channels, 128-byte internal RAM, 4-bit samples
struct Namco163 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    prg_ram: [u8; 8192],
    mirroring: Mirroring,
    prg_banks: [u8; 3],
    chr_banks: [u8; 12],
    // Internal 128-byte RAM (waveform data + channel registers)
    sound_ram: [u8; 128],
    sound_addr: u8,
    sound_auto_increment: bool,
    sound_disable: bool,
    // IRQ
    irq_counter: u16,
    irq_enabled: bool,
    irq_pending: bool,
    // Audio timing: one channel updated every 15 CPU cycles
    audio_timer: u8,
    audio_channel_index: u8,
    audio_output_level: f32,
}

impl Namco163 {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self {
            prg_rom,
            chr_rom,
            prg_ram: [0; 8192],
            mirroring,
            prg_banks: [0; 3],
            chr_banks: [0; 12],
            sound_ram: [0; 128],
            sound_addr: 0,
            sound_auto_increment: false,
            sound_disable: false,
            irq_counter: 0,
            irq_enabled: false,
            irq_pending: false,
            audio_timer: 0,
            audio_channel_index: 0,
            audio_output_level: 0.0,
        }
    }

    fn prg_8k_count(&self) -> usize {
        self.prg_rom.len() / 8192
    }

    fn read_prg_8k(&self, bank: usize, offset: usize) -> u8 {
        let bank = bank % self.prg_8k_count();
        self.prg_rom[bank * 8192 + offset]
    }

    fn num_active_channels(&self) -> u8 {
        // Channel count from address $7F bits 6-4
        ((self.sound_ram[0x7F] >> 4) & 0x07) + 1
    }

    #[allow(dead_code)]
    fn read_sound_ram(&mut self) -> u8 {
        let value = self.sound_ram[self.sound_addr as usize & 0x7F];
        if self.sound_auto_increment {
            self.sound_addr = (self.sound_addr + 1) & 0x7F;
        }
        value
    }

    fn write_sound_ram(&mut self, value: u8) {
        self.sound_ram[self.sound_addr as usize & 0x7F] = value;
        if self.sound_auto_increment {
            self.sound_addr = (self.sound_addr + 1) & 0x7F;
        }
    }

    /// Update one audio channel. Called every 15 CPU cycles, cycling through
    /// active channels in round-robin order.
    fn update_audio_channel(&mut self) {
        let num_channels = self.num_active_channels();
        let ch = self.audio_channel_index % num_channels;
        // Channel N is at base $78 - (num_channels - 1 - ch) * 8
        let base = 0x78 - (num_channels - 1 - ch) as usize * 8;

        // Read channel registers
        let freq_lo = self.sound_ram[base] as u32;
        let freq_mid = self.sound_ram[base + 2] as u32;
        let freq_hi = self.sound_ram[base + 4] as u32 & 0x03;
        let freq = freq_lo | (freq_mid << 8) | (freq_hi << 16);

        let wave_length_raw = (self.sound_ram[base + 4] >> 2) & 0x3F;
        let wave_length = (256 - u32::from(wave_length_raw) * 4) as u32;

        let wave_addr = self.sound_ram[base + 6] as u32;
        let volume = self.sound_ram[base + 7] & 0x0F;

        // Read 24-bit phase accumulator
        let phase_lo = self.sound_ram[base + 1] as u32;
        let phase_mid = self.sound_ram[base + 3] as u32;
        let phase_hi = self.sound_ram[base + 5] as u32;
        let mut phase = phase_lo | (phase_mid << 8) | (phase_hi << 16);

        // Advance phase
        phase = phase.wrapping_add(freq);
        if wave_length > 0 {
            let wrap_point = wave_length << 16;
            if phase >= wrap_point {
                phase %= wrap_point;
            }
        }

        // Write phase back
        self.sound_ram[base + 1] = phase as u8;
        self.sound_ram[base + 3] = (phase >> 8) as u8;
        self.sound_ram[base + 5] = (phase >> 16) as u8;

        // Look up waveform sample (4-bit, packed 2 per byte)
        let sample_index = ((phase >> 16) + wave_addr) & 0xFF;
        let ram_addr = (sample_index / 2) as usize & 0x7F;
        let sample = if sample_index & 1 == 0 {
            self.sound_ram[ram_addr] & 0x0F
        } else {
            (self.sound_ram[ram_addr] >> 4) & 0x0F
        };

        // Output: (sample - 8) * volume, range: -120 to +105
        let output = (i16::from(sample) - 8) * i16::from(volume);

        // Accumulate into output level (time-division multiplexed)
        self.audio_output_level = output as f32 / 120.0;

        self.audio_channel_index = (self.audio_channel_index + 1) % num_channels;
    }
}

impl Mapper for Namco163 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x4800..=0x4FFF => {
                // Sound RAM data port (non-mutating read for trait)
                self.sound_ram[self.sound_addr as usize & 0x7F]
            }
            0x5000..=0x57FF => {
                // IRQ counter low
                (self.irq_counter & 0xFF) as u8
            }
            0x5800..=0x5FFF => {
                // IRQ counter high + enable
                let hi = ((self.irq_counter >> 8) & 0x7F) as u8;
                hi | if self.irq_enabled { 0x80 } else { 0 }
            }
            0x6000..=0x7FFF => {
                self.prg_ram[(addr - 0x6000) as usize]
            }
            0x8000..=0x9FFF => {
                self.read_prg_8k(self.prg_banks[0] as usize & 0x3F, (addr - 0x8000) as usize)
            }
            0xA000..=0xBFFF => {
                self.read_prg_8k(self.prg_banks[1] as usize & 0x3F, (addr - 0xA000) as usize)
            }
            0xC000..=0xDFFF => {
                self.read_prg_8k(self.prg_banks[2] as usize & 0x3F, (addr - 0xC000) as usize)
            }
            0xE000..=0xFFFF => {
                self.read_prg_8k(self.prg_8k_count() - 1, (addr - 0xE000) as usize)
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x4800..=0x4FFF => {
                self.write_sound_ram(value);
            }
            0x5000..=0x57FF => {
                self.irq_counter = (self.irq_counter & 0xFF00) | u16::from(value);
                self.irq_pending = false;
            }
            0x5800..=0x5FFF => {
                self.irq_counter = (self.irq_counter & 0x00FF) | (u16::from(value & 0x7F) << 8);
                self.irq_enabled = value & 0x80 != 0;
                self.irq_pending = false;
            }
            0x6000..=0x7FFF => {
                self.prg_ram[(addr - 0x6000) as usize] = value;
            }
            // CHR bank registers ($8000-$DFFF, in $800 windows)
            0x8000..=0x87FF => self.chr_banks[0] = value,
            0x8800..=0x8FFF => self.chr_banks[1] = value,
            0x9000..=0x97FF => self.chr_banks[2] = value,
            0x9800..=0x9FFF => self.chr_banks[3] = value,
            0xA000..=0xA7FF => self.chr_banks[4] = value,
            0xA800..=0xAFFF => self.chr_banks[5] = value,
            0xB000..=0xB7FF => self.chr_banks[6] = value,
            0xB800..=0xBFFF => self.chr_banks[7] = value,
            0xC000..=0xC7FF => self.chr_banks[8] = value,
            0xC800..=0xCFFF => self.chr_banks[9] = value,
            0xD000..=0xD7FF => self.chr_banks[10] = value,
            0xD800..=0xDFFF => self.chr_banks[11] = value,
            // PRG bank 0 + sound disable
            0xE000..=0xE7FF => {
                self.prg_banks[0] = value & 0x3F;
                self.sound_disable = value & 0x40 != 0;
            }
            // PRG bank 1
            0xE800..=0xEFFF => {
                self.prg_banks[1] = value & 0x3F;
            }
            // PRG bank 2
            0xF000..=0xF7FF => {
                self.prg_banks[2] = value & 0x3F;
            }
            // Sound RAM address port
            0xF800..=0xFFFF => {
                self.sound_addr = value & 0x7F;
                self.sound_auto_increment = value & 0x80 != 0;
            }
            _ => {}
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        let addr_usize = addr as usize & 0x1FFF;
        let bank_index = addr_usize / 1024;

        // Nametable banks (indices 8-11) can map to CIRAM
        // Pattern table banks (0-7) always map to CHR-ROM
        let bank_value = self.chr_banks[bank_index] as usize;

        if bank_index >= 8 && bank_value >= 0xE0 {
            // CIRAM nametable — handled by PPU mirroring, return 0
            // (the PPU applies mirroring externally)
            return 0;
        }

        if self.chr_rom.is_empty() {
            return 0;
        }
        let offset = addr_usize & 0x3FF;
        let index = (bank_value * 1024 + offset) % self.chr_rom.len();
        self.chr_rom[index]
    }

    fn chr_write(&mut self, _addr: u16, _value: u8) {}

    fn mirroring(&self) -> Mirroring {
        // Nametable mirroring is controlled by chr_banks[8-11]
        // but the PPU needs a simple enum. Use the bank values to derive it.
        let nt0 = self.chr_banks[8];
        let nt1 = self.chr_banks[9];
        if nt0 == nt1 {
            Mirroring::SingleScreenLower
        } else if nt0 >= 0xE0 && nt1 >= 0xE0 {
            if nt0 & 1 == 0 && nt1 & 1 != 0 {
                Mirroring::Vertical
            } else {
                Mirroring::Horizontal
            }
        } else {
            self.mirroring
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn tick_audio(&mut self) {
        // IRQ counter: increments every CPU cycle, fires at $7FFF
        if self.irq_enabled {
            if self.irq_counter >= 0x7FFF {
                self.irq_pending = true;
            } else {
                self.irq_counter += 1;
            }
        }

        if self.sound_disable {
            self.audio_output_level = 0.0;
            return;
        }

        // Audio update: one channel every 15 CPU cycles
        self.audio_timer += 1;
        if self.audio_timer >= 15 {
            self.audio_timer = 0;
            self.update_audio_channel();
        }
    }

    fn audio_output(&self) -> f32 {
        if self.sound_disable {
            return 0.0;
        }
        // Scale to ~0.15 to balance with APU output
        self.audio_output_level * 0.15
    }

    fn prg_ram(&self) -> Option<&[u8]> {
        Some(&self.prg_ram)
    }

    fn set_prg_ram(&mut self, data: &[u8]) {
        let len = data.len().min(self.prg_ram.len());
        self.prg_ram[..len].copy_from_slice(&data[..len]);
    }
}

/// Bandai 74*161/02 (Mapper 70): 16K PRG + 8K CHR.
///
/// Used by Kamen Rider Club, Space Shadow. Simple banking with bus conflicts.
///
/// - PRG: 16K switchable at $8000, last 16K fixed
/// - CHR: 8K switchable
/// - Mirroring: hardwired
struct Bandai74161 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: Mirroring,
    prg_bank: u8,
    chr_bank: u8,
}

impl Bandai74161 {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self { prg_rom, chr_rom, mirroring, prg_bank: 0, chr_bank: 0 }
    }
    fn prg_16k_count(&self) -> usize { self.prg_rom.len() / 16384 }
}

impl Mapper for Bandai74161 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xBFFF => {
                let bank = self.prg_bank as usize % self.prg_16k_count();
                self.prg_rom[bank * 16384 + (addr - 0x8000) as usize]
            }
            0xC000..=0xFFFF => {
                let last = self.prg_16k_count().saturating_sub(1);
                self.prg_rom[last * 16384 + (addr - 0xC000) as usize]
            }
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            let effective = value & self.cpu_read(addr);
            self.prg_bank = (effective >> 4) & 0x0F;
            self.chr_bank = effective & 0x0F;
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        if self.chr_rom.is_empty() { return 0; }
        let idx = (self.chr_bank as usize * 8192 + (addr as usize & 0x1FFF)) % self.chr_rom.len();
        self.chr_rom[idx]
    }
    fn chr_write(&mut self, _addr: u16, _value: u8) {}
    fn mirroring(&self) -> Mirroring { self.mirroring }
}

/// Jaleco JF-17 (Mapper 72): acknowledge-then-write PRG/CHR switching.
///
/// Used by Pinball Quest, Moero!! Pro Tennis.
struct JalecoJf17 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: Mirroring,
    prg_bank: u8,
    chr_bank: u8,
    prg_latch: u8,
    chr_latch: u8,
}

impl JalecoJf17 {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self {
            prg_rom, chr_rom, mirroring,
            prg_bank: 0, chr_bank: 0, prg_latch: 0, chr_latch: 0,
        }
    }
    fn prg_16k_count(&self) -> usize { self.prg_rom.len() / 16384 }
}

impl Mapper for JalecoJf17 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xBFFF => {
                let bank = self.prg_bank as usize % self.prg_16k_count();
                self.prg_rom[bank * 16384 + (addr - 0x8000) as usize]
            }
            0xC000..=0xFFFF => {
                let last = self.prg_16k_count().saturating_sub(1);
                self.prg_rom[last * 16384 + (addr - 0xC000) as usize]
            }
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            let data = value & 0x0F;
            if value & 0x80 != 0 {
                self.prg_latch = data;
            }
            if value & 0x40 != 0 {
                self.chr_latch = data;
            }
            // Commit when acknowledge bits are cleared
            if value & 0xC0 == 0 {
                self.prg_bank = self.prg_latch;
                self.chr_bank = self.chr_latch;
            }
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        if self.chr_rom.is_empty() { return 0; }
        let idx = (self.chr_bank as usize * 8192 + (addr as usize & 0x1FFF)) % self.chr_rom.len();
        self.chr_rom[idx]
    }
    fn chr_write(&mut self, _addr: u16, _value: u8) {}
    fn mirroring(&self) -> Mirroring { self.mirroring }
}

/// Irem 74161 (Mapper 78): 16K PRG + 8K CHR + mirroring control.
///
/// Used by Holy Diver, Cosmo Carrier.
struct Irem74161 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: Mirroring,
    prg_bank: u8,
    chr_bank: u8,
    /// true = single-screen mode (Cosmo Carrier), false = H/V mode (Holy Diver)
    single_screen_mode: bool,
}

impl Irem74161 {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring, single_screen: bool) -> Self {
        Self {
            prg_rom, chr_rom, mirroring,
            prg_bank: 0, chr_bank: 0, single_screen_mode: single_screen,
        }
    }
    fn prg_16k_count(&self) -> usize { self.prg_rom.len() / 16384 }
}

impl Mapper for Irem74161 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xBFFF => {
                let bank = self.prg_bank as usize % self.prg_16k_count();
                self.prg_rom[bank * 16384 + (addr - 0x8000) as usize]
            }
            0xC000..=0xFFFF => {
                let last = self.prg_16k_count().saturating_sub(1);
                self.prg_rom[last * 16384 + (addr - 0xC000) as usize]
            }
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            self.prg_bank = value & 0x07;
            self.chr_bank = (value >> 4) & 0x0F;
            if self.single_screen_mode {
                self.mirroring = if value & 0x08 != 0 {
                    Mirroring::SingleScreenUpper
                } else {
                    Mirroring::SingleScreenLower
                };
            } else {
                self.mirroring = if value & 0x08 != 0 {
                    Mirroring::Vertical
                } else {
                    Mirroring::Horizontal
                };
            }
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        if self.chr_rom.is_empty() { return 0; }
        let idx = (self.chr_bank as usize * 8192 + (addr as usize & 0x1FFF)) % self.chr_rom.len();
        self.chr_rom[idx]
    }
    fn chr_write(&mut self, _addr: u16, _value: u8) {}
    fn mirroring(&self) -> Mirroring { self.mirroring }
}

/// Sunsoft-2 (Mapper 93): PRG-only switching.
///
/// Used by Fantasy Zone, Shanghai.
struct Sunsoft2 {
    prg_rom: Vec<u8>,
    chr_ram: [u8; 8192],
    mirroring: Mirroring,
    prg_bank: u8,
}

impl Sunsoft2 {
    fn new(prg_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self { prg_rom, chr_ram: [0; 8192], mirroring, prg_bank: 0 }
    }
    fn prg_16k_count(&self) -> usize { self.prg_rom.len() / 16384 }
}

impl Mapper for Sunsoft2 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xBFFF => {
                let bank = self.prg_bank as usize % self.prg_16k_count();
                self.prg_rom[bank * 16384 + (addr - 0x8000) as usize]
            }
            0xC000..=0xFFFF => {
                let last = self.prg_16k_count().saturating_sub(1);
                self.prg_rom[last * 16384 + (addr - 0xC000) as usize]
            }
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            self.prg_bank = (value >> 4) & 0x07;
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 { self.chr_ram[(addr as usize) & 0x1FFF] }
    fn chr_write(&mut self, addr: u16, value: u8) { self.chr_ram[(addr as usize) & 0x1FFF] = value; }
    fn mirroring(&self) -> Mirroring { self.mirroring }
}

/// Jaleco JF-11/14 (Mapper 140): 32K PRG + 8K CHR via $6000-$7FFF.
///
/// Used by Bio Senshi Dan, Murder on Mississippi.
struct JalecoJf11 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: Mirroring,
    prg_bank: u8,
    chr_bank: u8,
}

impl JalecoJf11 {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self { prg_rom, chr_rom, mirroring, prg_bank: 0, chr_bank: 0 }
    }
}

impl Mapper for JalecoJf11 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => {
                let bank_offset = self.prg_bank as usize * 32768;
                let idx = (bank_offset + (addr as usize - 0x8000)) % self.prg_rom.len();
                self.prg_rom[idx]
            }
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            self.prg_bank = (value >> 4) & 0x03;
            self.chr_bank = value & 0x0F;
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        if self.chr_rom.is_empty() { return 0; }
        let idx = (self.chr_bank as usize * 8192 + (addr as usize & 0x1FFF)) % self.chr_rom.len();
        self.chr_rom[idx]
    }
    fn chr_write(&mut self, _addr: u16, _value: u8) {}
    fn mirroring(&self) -> Mirroring { self.mirroring }
}

/// Bandai 74*161/02 with single-screen mirroring (Mapper 152).
///
/// Used by Arkanoid II, Saint Seiya.
struct Bandai74161Ss {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: Mirroring,
    prg_bank: u8,
    chr_bank: u8,
}

impl Bandai74161Ss {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>) -> Self {
        Self {
            prg_rom, chr_rom, mirroring: Mirroring::SingleScreenLower,
            prg_bank: 0, chr_bank: 0,
        }
    }
    fn prg_16k_count(&self) -> usize { self.prg_rom.len() / 16384 }
}

impl Mapper for Bandai74161Ss {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xBFFF => {
                let bank = self.prg_bank as usize % self.prg_16k_count();
                self.prg_rom[bank * 16384 + (addr - 0x8000) as usize]
            }
            0xC000..=0xFFFF => {
                let last = self.prg_16k_count().saturating_sub(1);
                self.prg_rom[last * 16384 + (addr - 0xC000) as usize]
            }
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            let effective = value & self.cpu_read(addr);
            self.prg_bank = (effective >> 4) & 0x07;
            self.chr_bank = effective & 0x0F;
            self.mirroring = if effective & 0x80 != 0 {
                Mirroring::SingleScreenUpper
            } else {
                Mirroring::SingleScreenLower
            };
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        if self.chr_rom.is_empty() { return 0; }
        let idx = (self.chr_bank as usize * 8192 + (addr as usize & 0x1FFF)) % self.chr_rom.len();
        self.chr_rom[idx]
    }
    fn chr_write(&mut self, _addr: u16, _value: u8) {}
    fn mirroring(&self) -> Mirroring { self.mirroring }
}

/// Sunsoft-1 (Mapper 184): CHR-only switching via $6000-$7FFF.
///
/// Used by Atlantis no Nazo, Wing of Madoola.
struct Sunsoft1 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: Mirroring,
    chr_bank_lo: u8,
    chr_bank_hi: u8,
}

impl Sunsoft1 {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self { prg_rom, chr_rom, mirroring, chr_bank_lo: 0, chr_bank_hi: 0 }
    }
}

impl Mapper for Sunsoft1 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => {
                self.prg_rom[(addr as usize - 0x8000) % self.prg_rom.len()]
            }
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            self.chr_bank_lo = value & 0x07;
            // High bank has bit 2 forced set (hardware wiring)
            self.chr_bank_hi = ((value >> 4) & 0x07) | 0x04;
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        if self.chr_rom.is_empty() { return 0; }
        let addr_usize = addr as usize & 0x1FFF;
        let bank = if addr_usize < 0x1000 {
            self.chr_bank_lo as usize
        } else {
            self.chr_bank_hi as usize
        };
        let offset = addr_usize & 0x0FFF;
        let idx = (bank * 4096 + offset) % self.chr_rom.len();
        self.chr_rom[idx]
    }
    fn chr_write(&mut self, _addr: u16, _value: u8) {}
    fn mirroring(&self) -> Mirroring { self.mirroring }
}

/// CNROM with copy protection (Mapper 185).
///
/// Used by Spy vs. Spy, Mighty Bomb Jack, B-Wings.
/// CHR ROM access gated: only returns data when CS bits match expected value.
struct CnromProtected {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: Mirroring,
    chr_enabled: bool,
}

impl CnromProtected {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self { prg_rom, chr_rom, mirroring, chr_enabled: false }
    }
}

impl Mapper for CnromProtected {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => self.prg_rom[(addr as usize - 0x8000) % self.prg_rom.len()],
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            // Protection: CS bits must be non-zero to enable CHR access
            self.chr_enabled = (value & 0x03) != 0;
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        if !self.chr_enabled || self.chr_rom.is_empty() {
            return 0xFF; // Open bus when protection active
        }
        self.chr_rom[(addr as usize) & 0x1FFF]
    }
    fn chr_write(&mut self, _addr: u16, _value: u8) {}
    fn mirroring(&self) -> Mirroring { self.mirroring }
}

/// Taito TC0690 (Mapper 48): like mapper 33 but with scanline IRQ.
///
/// Used by Don Doko Don 2, Flintstones: Rescue of Dino & Hoppy.
struct TaitoTc0690 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: Mirroring,
    prg_bank_0: u8,
    prg_bank_1: u8,
    chr_banks: [u8; 6],
    irq_latch: u8,
    irq_counter: u8,
    irq_reload_flag: bool,
    irq_enabled: bool,
    irq_pending: bool,
    last_a12: bool,
}

impl TaitoTc0690 {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self {
            prg_rom, chr_rom, mirroring,
            prg_bank_0: 0, prg_bank_1: 0, chr_banks: [0; 6],
            irq_latch: 0, irq_counter: 0, irq_reload_flag: false,
            irq_enabled: false, irq_pending: false, last_a12: false,
        }
    }
    fn prg_8k_count(&self) -> usize { self.prg_rom.len() / 8192 }
    fn read_prg_8k(&self, bank: usize, offset: usize) -> u8 {
        let bank = bank % self.prg_8k_count();
        self.prg_rom[bank * 8192 + offset]
    }
    fn read_chr(&self, bank: usize, offset: usize, bank_size: usize) -> u8 {
        if self.chr_rom.is_empty() { return 0; }
        let total = self.chr_rom.len() / bank_size;
        let bank = bank % total.max(1);
        self.chr_rom.get(bank * bank_size + offset).copied().unwrap_or(0)
    }
    fn clock_irq(&mut self, a12: bool) {
        if a12 && !self.last_a12 {
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
        self.last_a12 = a12;
    }
}

impl Mapper for TaitoTc0690 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0x9FFF => self.read_prg_8k(self.prg_bank_0 as usize & 0x3F, (addr - 0x8000) as usize),
            0xA000..=0xBFFF => self.read_prg_8k(self.prg_bank_1 as usize & 0x3F, (addr - 0xA000) as usize),
            0xC000..=0xDFFF => self.read_prg_8k(self.prg_8k_count() - 2, (addr - 0xC000) as usize),
            0xE000..=0xFFFF => self.read_prg_8k(self.prg_8k_count() - 1, (addr - 0xE000) as usize),
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x8000 => self.prg_bank_0 = value & 0x3F,
            0x8001 => self.prg_bank_1 = value & 0x3F,
            0x8002 => self.chr_banks[0] = value,
            0x8003 => self.chr_banks[1] = value,
            0xA000 => self.chr_banks[2] = value,
            0xA001 => self.chr_banks[3] = value,
            0xA002 => self.chr_banks[4] = value,
            0xA003 => self.chr_banks[5] = value,
            0xC000 => self.irq_latch = value ^ 0xFF,
            0xC001 => { self.irq_reload_flag = true; self.irq_pending = false; }
            0xC002 => self.irq_enabled = true,
            0xC003 => { self.irq_enabled = false; self.irq_pending = false; }
            0xE000 => {
                self.mirroring = if value & 0x40 != 0 { Mirroring::Horizontal } else { Mirroring::Vertical };
            }
            _ => {}
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        let a = addr as usize & 0x1FFF;
        self.clock_irq(a >= 0x1000);
        match a {
            0x0000..=0x07FF => self.read_chr(self.chr_banks[0] as usize, a & 0x7FF, 2048),
            0x0800..=0x0FFF => self.read_chr(self.chr_banks[1] as usize, a & 0x7FF, 2048),
            0x1000..=0x13FF => self.read_chr(self.chr_banks[2] as usize, a & 0x3FF, 1024),
            0x1400..=0x17FF => self.read_chr(self.chr_banks[3] as usize, a & 0x3FF, 1024),
            0x1800..=0x1BFF => self.read_chr(self.chr_banks[4] as usize, a & 0x3FF, 1024),
            0x1C00..=0x1FFF => self.read_chr(self.chr_banks[5] as usize, a & 0x3FF, 1024),
            _ => 0,
        }
    }
    fn chr_write(&mut self, _addr: u16, _value: u8) {}
    fn mirroring(&self) -> Mirroring { self.mirroring }
    fn irq_pending(&self) -> bool { self.irq_pending }
}

/// Sunsoft-3 (Mapper 67): 2K CHR banking with IRQ timer.
///
/// Used by Fantasy Zone II.
struct Sunsoft3 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: Mirroring,
    prg_bank: u8,
    chr_banks: [u8; 4],
    irq_counter: u16,
    irq_enabled: bool,
    irq_pending: bool,
    irq_toggle: bool,
}

impl Sunsoft3 {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self {
            prg_rom, chr_rom, mirroring, prg_bank: 0, chr_banks: [0; 4],
            irq_counter: 0, irq_enabled: false, irq_pending: false, irq_toggle: false,
        }
    }
    fn prg_16k_count(&self) -> usize { self.prg_rom.len() / 16384 }
}

impl Mapper for Sunsoft3 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xBFFF => {
                let bank = self.prg_bank as usize % self.prg_16k_count();
                self.prg_rom[bank * 16384 + (addr - 0x8000) as usize]
            }
            0xC000..=0xFFFF => {
                let last = self.prg_16k_count().saturating_sub(1);
                self.prg_rom[last * 16384 + (addr - 0xC000) as usize]
            }
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr & 0xF800 {
            0x8800 => self.chr_banks[0] = value,
            0x9800 => self.chr_banks[1] = value,
            0xA800 => self.chr_banks[2] = value,
            0xB800 => self.chr_banks[3] = value,
            0xC800 => {
                if self.irq_toggle {
                    self.irq_counter = (self.irq_counter & 0xFF00) | u16::from(value);
                } else {
                    self.irq_counter = (self.irq_counter & 0x00FF) | (u16::from(value) << 8);
                }
                self.irq_toggle = !self.irq_toggle;
            }
            0xD800 => {
                self.irq_enabled = value & 0x10 != 0;
                self.irq_toggle = false;
                self.irq_pending = false;
            }
            0xE800 => {
                self.mirroring = match value & 0x03 {
                    0 => Mirroring::Vertical,
                    1 => Mirroring::Horizontal,
                    2 => Mirroring::SingleScreenLower,
                    3 => Mirroring::SingleScreenUpper,
                    _ => unreachable!(),
                };
            }
            0xF800 => self.prg_bank = value & 0x0F,
            _ => {}
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        if self.chr_rom.is_empty() { return 0; }
        let a = addr as usize & 0x1FFF;
        let bank = self.chr_banks[a / 2048] as usize;
        let offset = a & 0x7FF;
        let idx = (bank * 2048 + offset) % self.chr_rom.len();
        self.chr_rom[idx]
    }
    fn chr_write(&mut self, _addr: u16, _value: u8) {}
    fn mirroring(&self) -> Mirroring { self.mirroring }
    fn irq_pending(&self) -> bool { self.irq_pending }
}

/// Sunsoft-4 (Mapper 68): 2K CHR + nametable ROM banking.
///
/// Used by After Burner (JP+US), Nantettatte!! Baseball.
struct Sunsoft4 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    prg_ram: [u8; 8192],
    mirroring: Mirroring,
    prg_bank: u8,
    chr_banks: [u8; 4],
    nt_banks: [u8; 2],
    nt_rom_mode: bool,
    prg_ram_enabled: bool,
}

impl Sunsoft4 {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self {
            prg_rom, chr_rom, prg_ram: [0; 8192], mirroring,
            prg_bank: 0, chr_banks: [0; 4], nt_banks: [0; 2],
            nt_rom_mode: false, prg_ram_enabled: false,
        }
    }
    fn prg_16k_count(&self) -> usize { self.prg_rom.len() / 16384 }
}

impl Mapper for Sunsoft4 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF if self.prg_ram_enabled => self.prg_ram[(addr - 0x6000) as usize],
            0x8000..=0xBFFF => {
                let bank = self.prg_bank as usize % self.prg_16k_count();
                self.prg_rom[bank * 16384 + (addr - 0x8000) as usize]
            }
            0xC000..=0xFFFF => {
                let last = self.prg_16k_count().saturating_sub(1);
                self.prg_rom[last * 16384 + (addr - 0xC000) as usize]
            }
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF if self.prg_ram_enabled => {
                self.prg_ram[(addr - 0x6000) as usize] = value;
            }
            0x8000..=0x8FFF => self.chr_banks[0] = value,
            0x9000..=0x9FFF => self.chr_banks[1] = value,
            0xA000..=0xAFFF => self.chr_banks[2] = value,
            0xB000..=0xBFFF => self.chr_banks[3] = value,
            0xC000..=0xCFFF => self.nt_banks[0] = value | 0x80,
            0xD000..=0xDFFF => self.nt_banks[1] = value | 0x80,
            0xE000..=0xEFFF => {
                self.nt_rom_mode = value & 0x10 != 0;
                self.mirroring = match value & 0x03 {
                    0 => Mirroring::Vertical,
                    1 => Mirroring::Horizontal,
                    2 => Mirroring::SingleScreenLower,
                    3 => Mirroring::SingleScreenUpper,
                    _ => unreachable!(),
                };
            }
            0xF000..=0xFFFF => {
                self.prg_bank = value & 0x0F;
                self.prg_ram_enabled = value & 0x80 != 0;
            }
            _ => {}
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        if self.chr_rom.is_empty() { return 0; }
        let a = addr as usize & 0x1FFF;
        let bank = self.chr_banks[a / 2048] as usize;
        let offset = a & 0x7FF;
        let idx = (bank * 2048 + offset) % self.chr_rom.len();
        self.chr_rom[idx]
    }
    fn chr_write(&mut self, _addr: u16, _value: u8) {}
    fn mirroring(&self) -> Mirroring { self.mirroring }
    fn prg_ram(&self) -> Option<&[u8]> { if self.prg_ram_enabled { Some(&self.prg_ram) } else { None } }
    fn set_prg_ram(&mut self, data: &[u8]) {
        let len = data.len().min(8192);
        self.prg_ram[..len].copy_from_slice(&data[..len]);
    }
}

/// VRC1 (Mapper 75): Konami 8K PRG + 4K CHR banking.
///
/// Used by Ganbare Goemon, Tetsuwan Atom.
struct Vrc1 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: Mirroring,
    prg_banks: [u8; 3],
    chr_bank_lo: u8,
    chr_bank_hi: u8,
    chr_hi_bits: [u8; 2],
}

impl Vrc1 {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self {
            prg_rom, chr_rom, mirroring, prg_banks: [0; 3],
            chr_bank_lo: 0, chr_bank_hi: 0, chr_hi_bits: [0; 2],
        }
    }
    fn prg_8k_count(&self) -> usize { self.prg_rom.len() / 8192 }
    fn read_prg_8k(&self, bank: usize, offset: usize) -> u8 {
        let bank = bank % self.prg_8k_count();
        self.prg_rom[bank * 8192 + offset]
    }
}

impl Mapper for Vrc1 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0x9FFF => self.read_prg_8k(self.prg_banks[0] as usize, (addr - 0x8000) as usize),
            0xA000..=0xBFFF => self.read_prg_8k(self.prg_banks[1] as usize, (addr - 0xA000) as usize),
            0xC000..=0xDFFF => self.read_prg_8k(self.prg_banks[2] as usize, (addr - 0xC000) as usize),
            0xE000..=0xFFFF => self.read_prg_8k(self.prg_8k_count() - 1, (addr - 0xE000) as usize),
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr & 0xF000 {
            0x8000 => self.prg_banks[0] = value & 0x0F,
            0x9000 => {
                self.mirroring = if value & 0x01 != 0 { Mirroring::Horizontal } else { Mirroring::Vertical };
                self.chr_hi_bits[0] = (value >> 1) & 0x01;
                self.chr_hi_bits[1] = (value >> 2) & 0x01;
            }
            0xA000 => self.prg_banks[1] = value & 0x0F,
            0xC000 => self.prg_banks[2] = value & 0x0F,
            0xE000 => self.chr_bank_lo = value & 0x0F,
            0xF000 => self.chr_bank_hi = value & 0x0F,
            _ => {}
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        if self.chr_rom.is_empty() { return 0; }
        let a = addr as usize & 0x1FFF;
        let bank = if a < 0x1000 {
            (self.chr_hi_bits[0] as usize) << 4 | self.chr_bank_lo as usize
        } else {
            (self.chr_hi_bits[1] as usize) << 4 | self.chr_bank_hi as usize
        };
        let offset = a & 0x0FFF;
        let idx = (bank * 4096 + offset) % self.chr_rom.len();
        self.chr_rom[idx]
    }
    fn chr_write(&mut self, _addr: u16, _value: u8) {}
    fn mirroring(&self) -> Mirroring { self.mirroring }
}

/// Taito X1-005 (Mapper 80): 8K PRG + mixed CHR banking via $7EF0-$7EFF.
///
/// Used by various Taito JP titles.
struct TaitoX1005 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: Mirroring,
    prg_banks: [u8; 3],
    chr_banks: [u8; 6],
    ram: [u8; 128],
    ram_enabled: bool,
}

impl TaitoX1005 {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self {
            prg_rom, chr_rom, mirroring, prg_banks: [0; 3],
            chr_banks: [0; 6], ram: [0; 128], ram_enabled: false,
        }
    }
    fn prg_8k_count(&self) -> usize { self.prg_rom.len() / 8192 }
    fn read_prg_8k(&self, bank: usize, offset: usize) -> u8 {
        let bank = bank % self.prg_8k_count();
        self.prg_rom[bank * 8192 + offset]
    }
}

impl Mapper for TaitoX1005 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x7F00..=0x7FFF if self.ram_enabled => self.ram[(addr & 0x7F) as usize],
            0x8000..=0x9FFF => self.read_prg_8k(self.prg_banks[0] as usize, (addr - 0x8000) as usize),
            0xA000..=0xBFFF => self.read_prg_8k(self.prg_banks[1] as usize, (addr - 0xA000) as usize),
            0xC000..=0xDFFF => self.read_prg_8k(self.prg_banks[2] as usize, (addr - 0xC000) as usize),
            0xE000..=0xFFFF => self.read_prg_8k(self.prg_8k_count() - 1, (addr - 0xE000) as usize),
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x7EF0 => self.chr_banks[0] = value & 0xFE, // 2K: ignore bit 0
            0x7EF1 => self.chr_banks[1] = value & 0xFE,
            0x7EF2 => self.chr_banks[2] = value,
            0x7EF3 => self.chr_banks[3] = value,
            0x7EF4 => self.chr_banks[4] = value,
            0x7EF5 => self.chr_banks[5] = value,
            0x7EF6 | 0x7EF7 => {
                self.mirroring = if value & 0x01 != 0 { Mirroring::Vertical } else { Mirroring::Horizontal };
            }
            0x7EF8 | 0x7EF9 => self.ram_enabled = value == 0xA3,
            0x7EFA | 0x7EFB => self.prg_banks[0] = value,
            0x7EFC | 0x7EFD => self.prg_banks[1] = value,
            0x7EFE | 0x7EFF => self.prg_banks[2] = value,
            0x7F00..=0x7FFF if self.ram_enabled => {
                self.ram[(addr & 0x7F) as usize] = value;
            }
            _ => {}
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        if self.chr_rom.is_empty() { return 0; }
        let a = addr as usize & 0x1FFF;
        let (bank, offset) = match a {
            0x0000..=0x07FF => (self.chr_banks[0] as usize, a),
            0x0800..=0x0FFF => (self.chr_banks[1] as usize, a - 0x0800),
            0x1000..=0x13FF => (self.chr_banks[2] as usize, a & 0x3FF),
            0x1400..=0x17FF => (self.chr_banks[3] as usize, a & 0x3FF),
            0x1800..=0x1BFF => (self.chr_banks[4] as usize, a & 0x3FF),
            0x1C00..=0x1FFF => (self.chr_banks[5] as usize, a & 0x3FF),
            _ => (0, 0),
        };
        // 2K banks (0,1) use 1K indexing but value was forced even
        let idx = if a < 0x1000 {
            (bank as usize * 1024 + offset) % self.chr_rom.len()
        } else {
            (bank as usize * 1024 + offset) % self.chr_rom.len()
        };
        self.chr_rom[idx]
    }
    fn chr_write(&mut self, _addr: u16, _value: u8) {}
    fn mirroring(&self) -> Mirroring { self.mirroring }
}

/// Taito X1-017 (Mapper 82): variant of X1-005 with CHR A12 inversion.
///
/// Used by Kyuukyoku Harikiri Stadium series.
struct TaitoX1017 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: Mirroring,
    prg_banks: [u8; 3],
    chr_banks: [u8; 6],
    chr_invert: bool,
    ram: [u8; 5120],
    ram_enabled: bool,
}

impl TaitoX1017 {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self {
            prg_rom, chr_rom, mirroring, prg_banks: [0; 3],
            chr_banks: [0; 6], chr_invert: false, ram: [0; 5120], ram_enabled: false,
        }
    }
    fn prg_8k_count(&self) -> usize { self.prg_rom.len() / 8192 }
    fn read_prg_8k(&self, bank: usize, offset: usize) -> u8 {
        let bank = bank % self.prg_8k_count();
        self.prg_rom[bank * 8192 + offset]
    }
}

impl Mapper for TaitoX1017 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x73FF if self.ram_enabled => self.ram[(addr - 0x6000) as usize],
            0x8000..=0x9FFF => self.read_prg_8k(self.prg_banks[0] as usize, (addr - 0x8000) as usize),
            0xA000..=0xBFFF => self.read_prg_8k(self.prg_banks[1] as usize, (addr - 0xA000) as usize),
            0xC000..=0xDFFF => self.read_prg_8k(self.prg_banks[2] as usize, (addr - 0xC000) as usize),
            0xE000..=0xFFFF => self.read_prg_8k(self.prg_8k_count() - 1, (addr - 0xE000) as usize),
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x73FF if self.ram_enabled => { self.ram[(addr - 0x6000) as usize] = value; }
            0x7EF0 => self.chr_banks[0] = value,
            0x7EF1 => self.chr_banks[1] = value,
            0x7EF2 => self.chr_banks[2] = value,
            0x7EF3 => self.chr_banks[3] = value,
            0x7EF4 => self.chr_banks[4] = value,
            0x7EF5 => self.chr_banks[5] = value,
            0x7EF6 => {
                self.mirroring = if value & 0x01 != 0 { Mirroring::Vertical } else { Mirroring::Horizontal };
                self.chr_invert = value & 0x02 != 0;
            }
            0x7EF7 => self.ram_enabled = value == 0xCA,
            0x7EF8 => { if value == 0x69 { self.ram_enabled = true; } }
            0x7EF9 => { if value == 0x84 { self.ram_enabled = true; } }
            0x7EFA => self.prg_banks[0] = value >> 2,
            0x7EFB => self.prg_banks[1] = value >> 2,
            0x7EFC => self.prg_banks[2] = value >> 2,
            _ => {}
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        if self.chr_rom.is_empty() { return 0; }
        let a = (addr as usize & 0x1FFF) ^ if self.chr_invert { 0x1000 } else { 0 };
        let (bank, offset, bank_size) = match a {
            0x0000..=0x07FF => (self.chr_banks[0] as usize >> 1, a & 0x7FF, 2048),
            0x0800..=0x0FFF => (self.chr_banks[1] as usize >> 1, a & 0x7FF, 2048),
            0x1000..=0x13FF => (self.chr_banks[2] as usize, a & 0x3FF, 1024),
            0x1400..=0x17FF => (self.chr_banks[3] as usize, a & 0x3FF, 1024),
            0x1800..=0x1BFF => (self.chr_banks[4] as usize, a & 0x3FF, 1024),
            0x1C00..=0x1FFF => (self.chr_banks[5] as usize, a & 0x3FF, 1024),
            _ => (0, 0, 1024),
        };
        let idx = (bank * bank_size + offset) % self.chr_rom.len();
        self.chr_rom[idx]
    }
    fn chr_write(&mut self, _addr: u16, _value: u8) {}
    fn mirroring(&self) -> Mirroring { self.mirroring }
}

/// Namco 3446 (Mapper 88): like mapper 206 but with CHR A16 split.
///
/// Used by Dragon Spirit, Quinty.
struct Namco3446 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: Mirroring,
    registers: [u8; 8],
    bank_select: u8,
}

impl Namco3446 {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self { prg_rom, chr_rom, mirroring, registers: [0; 8], bank_select: 0 }
    }
    fn prg_8k_count(&self) -> usize { self.prg_rom.len() / 8192 }
    fn read_prg_8k(&self, bank: usize, offset: usize) -> u8 {
        let bank = bank % self.prg_8k_count();
        self.prg_rom[bank * 8192 + offset]
    }
}

impl Mapper for Namco3446 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0x9FFF => self.read_prg_8k(self.registers[6] as usize & 0x0F, (addr - 0x8000) as usize),
            0xA000..=0xBFFF => self.read_prg_8k(self.registers[7] as usize & 0x0F, (addr - 0xA000) as usize),
            0xC000..=0xDFFF => self.read_prg_8k(self.prg_8k_count() - 2, (addr - 0xC000) as usize),
            0xE000..=0xFFFF => self.read_prg_8k(self.prg_8k_count() - 1, (addr - 0xE000) as usize),
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x8000 => self.bank_select = value & 0x07,
            0x8001 => {
                let reg = self.bank_select as usize;
                if reg < 8 {
                    // Registers 2-5: OR with $40 (access second 64K CHR half)
                    self.registers[reg] = if reg >= 2 && reg <= 5 {
                        (value & 0x3F) | 0x40
                    } else {
                        value
                    };
                }
            }
            _ => {}
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        if self.chr_rom.is_empty() { return 0; }
        let a = addr as usize & 0x1FFF;
        let (bank, offset) = match a {
            0x0000..=0x07FF => (self.registers[0] as usize & 0x3E, a),       // 2K
            0x0800..=0x0FFF => (self.registers[1] as usize & 0x3E, a & 0x7FF), // 2K
            0x1000..=0x13FF => (self.registers[2] as usize, a & 0x3FF),
            0x1400..=0x17FF => (self.registers[3] as usize, a & 0x3FF),
            0x1800..=0x1BFF => (self.registers[4] as usize, a & 0x3FF),
            0x1C00..=0x1FFF => (self.registers[5] as usize, a & 0x3FF),
            _ => (0, 0),
        };
        let idx = (bank * 1024 + offset) % self.chr_rom.len();
        self.chr_rom[idx]
    }
    fn chr_write(&mut self, _addr: u16, _value: u8) {}
    fn mirroring(&self) -> Mirroring { self.mirroring }
}

/// Namco 175/340 (Mapper 210): simpler Namco banking without audio/IRQ.
///
/// Used by Splatterhouse, Wagyan Land 2/3, Famista series.
struct Namco175 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    mirroring: Mirroring,
    prg_banks: [u8; 3],
    chr_banks: [u8; 8],
}

impl Namco175 {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self { prg_rom, chr_rom, mirroring, prg_banks: [0; 3], chr_banks: [0; 8] }
    }
    fn prg_8k_count(&self) -> usize { self.prg_rom.len() / 8192 }
    fn read_prg_8k(&self, bank: usize, offset: usize) -> u8 {
        let bank = bank % self.prg_8k_count();
        self.prg_rom[bank * 8192 + offset]
    }
}

impl Mapper for Namco175 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0x9FFF => self.read_prg_8k(self.prg_banks[0] as usize & 0x3F, (addr - 0x8000) as usize),
            0xA000..=0xBFFF => self.read_prg_8k(self.prg_banks[1] as usize & 0x3F, (addr - 0xA000) as usize),
            0xC000..=0xDFFF => self.read_prg_8k(self.prg_banks[2] as usize & 0x3F, (addr - 0xC000) as usize),
            0xE000..=0xFFFF => self.read_prg_8k(self.prg_8k_count() - 1, (addr - 0xE000) as usize),
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr & 0xF800 {
            0x8000 => self.chr_banks[0] = value,
            0x8800 => self.chr_banks[1] = value,
            0x9000 => self.chr_banks[2] = value,
            0x9800 => self.chr_banks[3] = value,
            0xA000 => self.chr_banks[4] = value,
            0xA800 => self.chr_banks[5] = value,
            0xB000 => self.chr_banks[6] = value,
            0xB800 => self.chr_banks[7] = value,
            0xE000 => {
                self.prg_banks[0] = value & 0x3F;
                // Submapper 2 (Namco 340): bits 7-6 control mirroring
                self.mirroring = match (value >> 6) & 0x03 {
                    0 => Mirroring::SingleScreenLower,
                    1 => Mirroring::Vertical,
                    2 => Mirroring::SingleScreenUpper,
                    3 => Mirroring::Horizontal,
                    _ => unreachable!(),
                };
            }
            0xE800 => self.prg_banks[1] = value & 0x3F,
            0xF000 => self.prg_banks[2] = value & 0x3F,
            _ => {}
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        if self.chr_rom.is_empty() { return 0; }
        let a = addr as usize & 0x1FFF;
        let bank = self.chr_banks[a / 1024] as usize;
        let offset = a & 0x3FF;
        let idx = (bank * 1024 + offset) % self.chr_rom.len();
        self.chr_rom[idx]
    }
    fn chr_write(&mut self, _addr: u16, _value: u8) {}
    fn mirroring(&self) -> Mirroring { self.mirroring }
}

/// VRC6 expansion audio pulse channel.
struct Vrc6Pulse {
    duty: u8,
    volume: u8,
    mode: bool,   // true = constant volume (ignore duty)
    enabled: bool,
    period: u16,
    counter: u16,
    step: u8,
}

impl Vrc6Pulse {
    fn new() -> Self {
        Self { duty: 0, volume: 0, mode: false, enabled: false, period: 0, counter: 0, step: 0 }
    }
    fn tick(&mut self) {
        if !self.enabled { return; }
        if self.counter == 0 {
            self.counter = self.period;
            self.step = (self.step + 1) & 0x0F;
        } else {
            self.counter -= 1;
        }
    }
    fn output(&self) -> u8 {
        if !self.enabled { return 0; }
        if self.mode || self.step <= self.duty { self.volume } else { 0 }
    }
}

/// VRC6 expansion audio sawtooth channel.
struct Vrc6Sawtooth {
    rate: u8,
    enabled: bool,
    period: u16,
    counter: u16,
    step: u8,
    accumulator: u8,
}

impl Vrc6Sawtooth {
    fn new() -> Self {
        Self { rate: 0, enabled: false, period: 0, counter: 0, step: 0, accumulator: 0 }
    }
    fn tick(&mut self) {
        if !self.enabled { return; }
        if self.counter == 0 {
            self.counter = self.period;
            self.step += 1;
            if self.step >= 14 {
                self.step = 0;
                self.accumulator = 0;
            } else if self.step & 1 == 0 {
                self.accumulator = self.accumulator.wrapping_add(self.rate);
            }
        } else {
            self.counter -= 1;
        }
    }
    fn output(&self) -> u8 {
        if !self.enabled { return 0; }
        self.accumulator >> 3 // Top 5 bits
    }
}

/// Konami VRC6 (Mapper 24/26): 8K/16K PRG + 1K CHR + IRQ + expansion audio.
///
/// Used by Castlevania III (JP: Akumajou Densetsu), Esper Dream 2, Madara.
///
/// Expansion audio: 2 pulse channels + 1 sawtooth channel.
struct Vrc6 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    chr_is_ram: bool,
    prg_ram: [u8; 8192],
    mirroring: Mirroring,
    prg_16k: u8,
    prg_8k: u8,
    chr_banks: [u8; 8],
    prg_ram_enabled: bool,
    // Address line swap: mapper 24 = normal, mapper 26 = A0/A1 swapped
    addr_swap: bool,
    // IRQ
    irq_latch: u8,
    irq_counter: u8,
    irq_prescaler: i16,
    irq_enabled: bool,
    irq_enabled_after_ack: bool,
    irq_mode_cycle: bool,
    irq_pending: bool,
    // Audio
    pulse1: Vrc6Pulse,
    pulse2: Vrc6Pulse,
    sawtooth: Vrc6Sawtooth,
    audio_halt: bool,
}

impl Vrc6 {
    fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>, mirroring: Mirroring, addr_swap: bool) -> Self {
        let chr_is_ram = chr_data.is_empty();
        let chr_rom = if chr_is_ram { vec![0u8; 8192] } else { chr_data };
        Self {
            prg_rom, chr_rom, chr_is_ram, prg_ram: [0; 8192], mirroring,
            prg_16k: 0, prg_8k: 0, chr_banks: [0; 8], prg_ram_enabled: false,
            addr_swap,
            irq_latch: 0, irq_counter: 0, irq_prescaler: 341,
            irq_enabled: false, irq_enabled_after_ack: false,
            irq_mode_cycle: false, irq_pending: false,
            pulse1: Vrc6Pulse::new(), pulse2: Vrc6Pulse::new(),
            sawtooth: Vrc6Sawtooth::new(), audio_halt: false,
        }
    }
    fn prg_16k_count(&self) -> usize { self.prg_rom.len() / 16384 }
    fn prg_8k_count(&self) -> usize { self.prg_rom.len() / 8192 }
    fn remap_addr(&self, addr: u16) -> u16 {
        if self.addr_swap {
            let base = addr & 0xFFFC;
            let a0 = (addr >> 1) & 1;
            let a1 = (addr & 1) << 1;
            base | a1 | a0
        } else {
            addr
        }
    }
}

impl Mapper for Vrc6 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF if self.prg_ram_enabled => self.prg_ram[(addr - 0x6000) as usize],
            0x8000..=0xBFFF => {
                let bank = self.prg_16k as usize % self.prg_16k_count();
                self.prg_rom[bank * 16384 + (addr - 0x8000) as usize]
            }
            0xC000..=0xDFFF => {
                let bank = self.prg_8k as usize % self.prg_8k_count();
                self.prg_rom[bank * 8192 + (addr - 0xC000) as usize]
            }
            0xE000..=0xFFFF => {
                let last = self.prg_8k_count() - 1;
                self.prg_rom[last * 8192 + (addr - 0xE000) as usize]
            }
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        let mapped = self.remap_addr(addr);
        match mapped {
            0x6000..=0x7FFF if self.prg_ram_enabled => {
                self.prg_ram[(addr - 0x6000) as usize] = value;
            }
            0x8000..=0x8003 => self.prg_16k = value & 0x0F,
            0x9000 => {
                self.pulse1.duty = (value >> 4) & 0x07;
                self.pulse1.volume = value & 0x0F;
                self.pulse1.mode = value & 0x80 != 0;
            }
            0x9001 => self.pulse1.period = (self.pulse1.period & 0x0F00) | u16::from(value),
            0x9002 => {
                self.pulse1.period = (self.pulse1.period & 0x00FF) | (u16::from(value & 0x0F) << 8);
                self.pulse1.enabled = value & 0x80 != 0;
            }
            0x9003 => {
                self.audio_halt = value & 0x01 != 0;
            }
            0xA000 => {
                self.pulse2.duty = (value >> 4) & 0x07;
                self.pulse2.volume = value & 0x0F;
                self.pulse2.mode = value & 0x80 != 0;
            }
            0xA001 => self.pulse2.period = (self.pulse2.period & 0x0F00) | u16::from(value),
            0xA002 => {
                self.pulse2.period = (self.pulse2.period & 0x00FF) | (u16::from(value & 0x0F) << 8);
                self.pulse2.enabled = value & 0x80 != 0;
            }
            0xB000 => self.sawtooth.rate = value & 0x3F,
            0xB001 => self.sawtooth.period = (self.sawtooth.period & 0x0F00) | u16::from(value),
            0xB002 => {
                self.sawtooth.period = (self.sawtooth.period & 0x00FF) | (u16::from(value & 0x0F) << 8);
                self.sawtooth.enabled = value & 0x80 != 0;
            }
            0xB003 => {
                self.mirroring = match (value >> 2) & 0x03 {
                    0 => Mirroring::Vertical,
                    1 => Mirroring::Horizontal,
                    2 => Mirroring::SingleScreenLower,
                    3 => Mirroring::SingleScreenUpper,
                    _ => unreachable!(),
                };
                self.prg_ram_enabled = value & 0x80 != 0;
            }
            0xC000..=0xC003 => self.prg_8k = value & 0x1F,
            0xD000..=0xD003 => self.chr_banks[(mapped & 3) as usize] = value,
            0xE000..=0xE003 => self.chr_banks[4 + (mapped & 3) as usize] = value,
            0xF000 => self.irq_latch = value,
            0xF001 => {
                self.irq_pending = false;
                self.irq_enabled_after_ack = value & 0x01 != 0;
                self.irq_enabled = value & 0x02 != 0;
                self.irq_mode_cycle = value & 0x04 != 0;
                if self.irq_enabled {
                    self.irq_counter = self.irq_latch;
                    self.irq_prescaler = 341;
                }
            }
            0xF002 => {
                self.irq_pending = false;
                self.irq_enabled = self.irq_enabled_after_ack;
            }
            _ => {}
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        let a = addr as usize & 0x1FFF;
        let bank = self.chr_banks[a / 1024] as usize;
        let offset = a & 0x3FF;
        let idx = (bank * 1024 + offset) % self.chr_rom.len();
        self.chr_rom[idx]
    }
    fn chr_write(&mut self, addr: u16, value: u8) {
        if self.chr_is_ram { self.chr_rom[(addr as usize) & 0x1FFF] = value; }
    }
    fn mirroring(&self) -> Mirroring { self.mirroring }
    fn irq_pending(&self) -> bool { self.irq_pending }
    fn tick_audio(&mut self) {
        if self.audio_halt { return; }
        self.pulse1.tick();
        self.pulse2.tick();
        self.sawtooth.tick();
    }
    fn audio_output(&self) -> f32 {
        if self.audio_halt { return 0.0; }
        let p1 = f32::from(self.pulse1.output());
        let p2 = f32::from(self.pulse2.output());
        let saw = f32::from(self.sawtooth.output());
        // Scale to ~0.15 total to balance with APU
        (p1 + p2 + saw) / 45.0 * 0.15
    }
    fn prg_ram(&self) -> Option<&[u8]> {
        if self.prg_ram_enabled { Some(&self.prg_ram) } else { None }
    }
    fn set_prg_ram(&mut self, data: &[u8]) {
        let len = data.len().min(8192);
        self.prg_ram[..len].copy_from_slice(&data[..len]);
    }
}

/// TxSROM (Mapper 118): MMC3 variant where CHR bank bit 7 controls nametable mirroring.
///
/// Used by Ys III, NES Play Action Football, Goal! Two, Armadillo.
struct TxSrom {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    prg_ram: [u8; 8192],
    bank_select: u8,
    registers: [u8; 8],
    irq_latch: u8,
    irq_counter: u8,
    irq_reload_flag: bool,
    irq_enabled: bool,
    irq_pending: bool,
    last_a12: bool,
}

impl TxSrom {
    fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>) -> Self {
        let chr_is_ram = chr_data.is_empty();
        let chr = if chr_is_ram { vec![0u8; 8192] } else { chr_data };
        Self {
            prg_rom, chr, chr_is_ram, prg_ram: [0; 8192],
            bank_select: 0, registers: [0; 8],
            irq_latch: 0, irq_counter: 0, irq_reload_flag: false,
            irq_enabled: false, irq_pending: false, last_a12: false,
        }
    }
    fn prg_8k_count(&self) -> usize { self.prg_rom.len() / 8192 }
    fn read_prg_8k(&self, bank: usize, offset: usize) -> u8 {
        let bank = bank % self.prg_8k_count();
        self.prg_rom[bank * 8192 + offset]
    }
}

impl Mapper for TxSrom {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.prg_ram[(addr - 0x6000) as usize],
            0x8000..=0x9FFF => {
                let bank = if self.bank_select & 0x40 != 0 {
                    self.prg_8k_count() - 2
                } else {
                    self.registers[6] as usize & 0x3F
                };
                self.read_prg_8k(bank, (addr - 0x8000) as usize)
            }
            0xA000..=0xBFFF => self.read_prg_8k(self.registers[7] as usize & 0x3F, (addr - 0xA000) as usize),
            0xC000..=0xDFFF => {
                let bank = if self.bank_select & 0x40 != 0 {
                    self.registers[6] as usize & 0x3F
                } else {
                    self.prg_8k_count() - 2
                };
                self.read_prg_8k(bank, (addr - 0xC000) as usize)
            }
            0xE000..=0xFFFF => self.read_prg_8k(self.prg_8k_count() - 1, (addr - 0xE000) as usize),
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => self.prg_ram[(addr - 0x6000) as usize] = value,
            0x8000..=0x9FFE if addr & 1 == 0 => self.bank_select = value,
            0x8001..=0x9FFF if addr & 1 == 1 => {
                let reg = (self.bank_select & 0x07) as usize;
                self.registers[reg] = value;
            }
            // $A000 mirroring is ignored on TxSROM — mirroring comes from CHR bits
            0xC000..=0xDFFE if addr & 1 == 0 => self.irq_latch = value,
            0xC001..=0xDFFF if addr & 1 == 1 => self.irq_reload_flag = true,
            0xE000..=0xFFFE if addr & 1 == 0 => { self.irq_enabled = false; self.irq_pending = false; }
            0xE001..=0xFFFF if addr & 1 == 1 => self.irq_enabled = true,
            _ => {}
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        let a = addr as usize & 0x1FFF;
        let a12 = a >= 0x1000;
        // Clock IRQ on A12 rising edge
        if a12 && !self.last_a12 {
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
        self.last_a12 = a12;
        if self.chr.is_empty() { return 0; }
        // Standard MMC3 CHR banking
        let chr_mode = self.bank_select & 0x80 != 0;
        let base = if chr_mode { 0x1000 } else { 0 };
        let effective = a ^ base;
        let bank = match effective {
            0x0000..=0x03FF => self.registers[0] as usize & 0xFE,
            0x0400..=0x07FF => self.registers[0] as usize | 1,
            0x0800..=0x0BFF => self.registers[1] as usize & 0xFE,
            0x0C00..=0x0FFF => self.registers[1] as usize | 1,
            0x1000..=0x13FF => self.registers[2] as usize,
            0x1400..=0x17FF => self.registers[3] as usize,
            0x1800..=0x1BFF => self.registers[4] as usize,
            0x1C00..=0x1FFF => self.registers[5] as usize,
            _ => 0,
        };
        let offset = a & 0x3FF;
        let idx = (bank * 1024 + offset) % self.chr.len();
        self.chr[idx]
    }
    fn chr_write(&mut self, addr: u16, value: u8) {
        if self.chr_is_ram { self.chr[(addr as usize) & 0x1FFF] = value; }
    }
    fn mirroring(&self) -> Mirroring {
        // Mirroring derived from CHR bank bit 7 for the nametable region
        // Use R0 bit 7 for a simple approximation
        let chr_mode = self.bank_select & 0x80 != 0;
        let nt_bank = if chr_mode { self.registers[2] } else { self.registers[0] };
        if nt_bank & 0x80 != 0 {
            Mirroring::SingleScreenUpper
        } else {
            Mirroring::SingleScreenLower
        }
    }
    fn irq_pending(&self) -> bool { self.irq_pending }
    fn prg_ram(&self) -> Option<&[u8]> { Some(&self.prg_ram) }
    fn set_prg_ram(&mut self, data: &[u8]) {
        let len = data.len().min(8192);
        self.prg_ram[..len].copy_from_slice(&data[..len]);
    }
}

/// TQROM (Mapper 119): MMC3 with both CHR ROM and CHR RAM, selected by bit 6.
///
/// Used by High Speed, Pin-Bot.
struct Tqrom {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    chr_ram: [u8; 8192],
    prg_ram: [u8; 8192],
    bank_select: u8,
    registers: [u8; 8],
    mirroring: Mirroring,
    irq_latch: u8,
    irq_counter: u8,
    irq_reload_flag: bool,
    irq_enabled: bool,
    irq_pending: bool,
    last_a12: bool,
}

impl Tqrom {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>) -> Self {
        Self {
            prg_rom, chr_rom, chr_ram: [0; 8192], prg_ram: [0; 8192],
            bank_select: 0, registers: [0; 8], mirroring: Mirroring::Vertical,
            irq_latch: 0, irq_counter: 0, irq_reload_flag: false,
            irq_enabled: false, irq_pending: false, last_a12: false,
        }
    }
    fn prg_8k_count(&self) -> usize { self.prg_rom.len() / 8192 }
    fn read_prg_8k(&self, bank: usize, offset: usize) -> u8 {
        let bank = bank % self.prg_8k_count();
        self.prg_rom[bank * 8192 + offset]
    }
}

impl Mapper for Tqrom {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.prg_ram[(addr - 0x6000) as usize],
            0x8000..=0x9FFF => {
                let bank = if self.bank_select & 0x40 != 0 { self.prg_8k_count() - 2 } else { self.registers[6] as usize & 0x3F };
                self.read_prg_8k(bank, (addr - 0x8000) as usize)
            }
            0xA000..=0xBFFF => self.read_prg_8k(self.registers[7] as usize & 0x3F, (addr - 0xA000) as usize),
            0xC000..=0xDFFF => {
                let bank = if self.bank_select & 0x40 != 0 { self.registers[6] as usize & 0x3F } else { self.prg_8k_count() - 2 };
                self.read_prg_8k(bank, (addr - 0xC000) as usize)
            }
            0xE000..=0xFFFF => self.read_prg_8k(self.prg_8k_count() - 1, (addr - 0xE000) as usize),
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => self.prg_ram[(addr - 0x6000) as usize] = value,
            0x8000..=0x9FFE if addr & 1 == 0 => self.bank_select = value,
            0x8001..=0x9FFF if addr & 1 == 1 => {
                self.registers[(self.bank_select & 0x07) as usize] = value;
            }
            0xA000..=0xBFFE if addr & 1 == 0 => {
                self.mirroring = if value & 0x01 != 0 { Mirroring::Horizontal } else { Mirroring::Vertical };
            }
            0xC000..=0xDFFE if addr & 1 == 0 => self.irq_latch = value,
            0xC001..=0xDFFF if addr & 1 == 1 => self.irq_reload_flag = true,
            0xE000..=0xFFFE if addr & 1 == 0 => { self.irq_enabled = false; self.irq_pending = false; }
            0xE001..=0xFFFF if addr & 1 == 1 => self.irq_enabled = true,
            _ => {}
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        let a = addr as usize & 0x1FFF;
        let a12 = a >= 0x1000;
        if a12 && !self.last_a12 {
            if self.irq_counter == 0 || self.irq_reload_flag {
                self.irq_counter = self.irq_latch;
                self.irq_reload_flag = false;
            } else {
                self.irq_counter -= 1;
            }
            if self.irq_counter == 0 && self.irq_enabled { self.irq_pending = true; }
        }
        self.last_a12 = a12;
        let chr_mode = self.bank_select & 0x80 != 0;
        let base = if chr_mode { 0x1000 } else { 0 };
        let effective = a ^ base;
        let bank = match effective {
            0x0000..=0x03FF => self.registers[0] as usize & 0xFE,
            0x0400..=0x07FF => self.registers[0] as usize | 1,
            0x0800..=0x0BFF => self.registers[1] as usize & 0xFE,
            0x0C00..=0x0FFF => self.registers[1] as usize | 1,
            0x1000..=0x13FF => self.registers[2] as usize,
            0x1400..=0x17FF => self.registers[3] as usize,
            0x1800..=0x1BFF => self.registers[4] as usize,
            0x1C00..=0x1FFF => self.registers[5] as usize,
            _ => 0,
        };
        // Bit 6: 0 = CHR ROM, 1 = CHR RAM
        if bank & 0x40 != 0 {
            self.chr_ram[((bank & 0x07) * 1024 + (a & 0x3FF)) & 0x1FFF]
        } else if self.chr_rom.is_empty() {
            0
        } else {
            let idx = ((bank & 0x3F) * 1024 + (a & 0x3FF)) % self.chr_rom.len();
            self.chr_rom[idx]
        }
    }
    fn chr_write(&mut self, addr: u16, value: u8) {
        // Writes only to CHR RAM banks
        let a = addr as usize & 0x1FFF;
        let chr_mode = self.bank_select & 0x80 != 0;
        let base = if chr_mode { 0x1000 } else { 0 };
        let effective = a ^ base;
        let bank = match effective {
            0x0000..=0x03FF => self.registers[0] as usize & 0xFE,
            0x0400..=0x07FF => self.registers[0] as usize | 1,
            0x0800..=0x0BFF => self.registers[1] as usize & 0xFE,
            0x0C00..=0x0FFF => self.registers[1] as usize | 1,
            0x1000..=0x13FF => self.registers[2] as usize,
            0x1400..=0x17FF => self.registers[3] as usize,
            0x1800..=0x1BFF => self.registers[4] as usize,
            0x1C00..=0x1FFF => self.registers[5] as usize,
            _ => 0,
        };
        if bank & 0x40 != 0 {
            self.chr_ram[((bank & 0x07) * 1024 + (a & 0x3FF)) & 0x1FFF] = value;
        }
    }
    fn mirroring(&self) -> Mirroring { self.mirroring }
    fn irq_pending(&self) -> bool { self.irq_pending }
}

/// Konami VRC7 (Mapper 85): 8K PRG + 1K CHR + IRQ + OPLL FM audio.
///
/// Used by Lagrange Point, Tiny Toon Adventures 2 (JP).
///
/// The VRC7 contains a YM2413 OPLL subset (6 channels, 15 built-in
/// instruments + 1 custom). FM synthesis is not yet implemented — register
/// writes are accepted but audio output is silent. Banking and IRQ work.
struct Vrc7 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    chr_is_ram: bool,
    prg_ram: [u8; 8192],
    mirroring: Mirroring,
    prg_banks: [u8; 3],
    chr_banks: [u8; 8],
    prg_ram_enabled: bool,
    audio_silenced: bool,
    // OPLL registers (accepted but not synthesised)
    #[allow(dead_code)]
    opll_addr: u8,
    #[allow(dead_code)]
    opll_regs: [u8; 64],
    // IRQ (same as VRC4/VRC6)
    irq_latch: u8,
    irq_counter: u8,
    irq_prescaler: i16,
    irq_enabled: bool,
    irq_enabled_after_ack: bool,
    irq_mode_cycle: bool,
    irq_pending: bool,
}

impl Vrc7 {
    fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>, mirroring: Mirroring) -> Self {
        let chr_is_ram = chr_data.is_empty();
        let chr_rom = if chr_is_ram { vec![0u8; 8192] } else { chr_data };
        Self {
            prg_rom, chr_rom, chr_is_ram, prg_ram: [0; 8192], mirroring,
            prg_banks: [0; 3], chr_banks: [0; 8],
            prg_ram_enabled: false, audio_silenced: false,
            opll_addr: 0, opll_regs: [0; 64],
            irq_latch: 0, irq_counter: 0, irq_prescaler: 341,
            irq_enabled: false, irq_enabled_after_ack: false,
            irq_mode_cycle: false, irq_pending: false,
        }
    }
    fn prg_8k_count(&self) -> usize { self.prg_rom.len() / 8192 }
    fn read_prg_8k(&self, bank: usize, offset: usize) -> u8 {
        let bank = bank % self.prg_8k_count();
        self.prg_rom[bank * 8192 + offset]
    }
}

impl Mapper for Vrc7 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF if self.prg_ram_enabled => self.prg_ram[(addr - 0x6000) as usize],
            0x8000..=0x9FFF => self.read_prg_8k(self.prg_banks[0] as usize, (addr - 0x8000) as usize),
            0xA000..=0xBFFF => self.read_prg_8k(self.prg_banks[1] as usize, (addr - 0xA000) as usize),
            0xC000..=0xDFFF => self.read_prg_8k(self.prg_banks[2] as usize, (addr - 0xC000) as usize),
            0xE000..=0xFFFF => self.read_prg_8k(self.prg_8k_count() - 1, (addr - 0xE000) as usize),
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        // VRC7 decodes A0, A4, A12-A15
        let reg = (addr & 0xF000) | ((addr & 0x0010) >> 4);
        match reg {
            0x8000 => self.prg_banks[0] = value & 0x3F,
            0x8001 => self.prg_banks[1] = value & 0x3F,
            0x9000 => self.prg_banks[2] = value & 0x3F,
            0x9001 => {
                // $9010: OPLL register select; $9030: OPLL data write
                // Both map to reg 0x9001 after our address decode. Distinguish
                // by original address.
                if addr & 0x0030 == 0x0010 {
                    self.opll_addr = value & 0x3F;
                } else {
                    // Data write (synthesis not implemented)
                    if (self.opll_addr as usize) < self.opll_regs.len() {
                        self.opll_regs[self.opll_addr as usize] = value;
                    }
                }
            }
            0xA000 => self.chr_banks[0] = value,
            0xA001 => self.chr_banks[1] = value,
            0xB000 => self.chr_banks[2] = value,
            0xB001 => self.chr_banks[3] = value,
            0xC000 => self.chr_banks[4] = value,
            0xC001 => self.chr_banks[5] = value,
            0xD000 => self.chr_banks[6] = value,
            0xD001 => self.chr_banks[7] = value,
            0xE000 => {
                self.mirroring = match value & 0x03 {
                    0 => Mirroring::Vertical,
                    1 => Mirroring::Horizontal,
                    2 => Mirroring::SingleScreenLower,
                    3 => Mirroring::SingleScreenUpper,
                    _ => unreachable!(),
                };
                self.audio_silenced = value & 0x40 != 0;
                self.prg_ram_enabled = value & 0x80 != 0;
            }
            0xE001 => self.irq_latch = value,
            0xF000 => {
                self.irq_pending = false;
                self.irq_enabled_after_ack = value & 0x01 != 0;
                self.irq_enabled = value & 0x02 != 0;
                self.irq_mode_cycle = value & 0x04 != 0;
                if self.irq_enabled {
                    self.irq_counter = self.irq_latch;
                    self.irq_prescaler = 341;
                }
            }
            0xF001 => {
                self.irq_pending = false;
                self.irq_enabled = self.irq_enabled_after_ack;
            }
            _ => {}
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        let a = addr as usize & 0x1FFF;
        let bank = self.chr_banks[a / 1024] as usize;
        let offset = a & 0x3FF;
        let idx = (bank * 1024 + offset) % self.chr_rom.len();
        self.chr_rom[idx]
    }
    fn chr_write(&mut self, addr: u16, value: u8) {
        if self.chr_is_ram { self.chr_rom[(addr as usize) & 0x1FFF] = value; }
    }
    fn mirroring(&self) -> Mirroring { self.mirroring }
    fn irq_pending(&self) -> bool { self.irq_pending }
    fn prg_ram(&self) -> Option<&[u8]> { if self.prg_ram_enabled { Some(&self.prg_ram) } else { None } }
    fn set_prg_ram(&mut self, data: &[u8]) {
        let len = data.len().min(8192);
        self.prg_ram[..len].copy_from_slice(&data[..len]);
    }
}

/// MMC5 (Mapper 5, ExROM): the most complex NES mapper.
///
/// Used by Castlevania III, Just Breed, Metal Slader Glory, L'Empereur,
/// Nobunaga's Ambition II, Romance of the Three Kingdoms II.
///
/// Implements: PRG banking (4 modes), CHR banking (2 modes with 8K sprite
/// and 4K background pages), 1K ExRAM, fill-mode nametable, PRG RAM banking,
/// vertical split (stub), and 8×8 multiplier. Scanline counter and
/// expansion audio (2 pulse channels) are not yet implemented.
struct Mmc5 {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    prg_ram: Vec<u8>,
    // PRG banking
    prg_mode: u8,
    prg_banks: [u8; 5],
    prg_ram_protect_1: u8,
    prg_ram_protect_2: u8,
    // CHR banking
    chr_mode: u8,
    chr_banks: [u16; 12],
    chr_upper: u8,
    last_chr_write_sprite: bool,
    // Nametable
    nametable_mapping: u8,
    fill_tile: u8,
    fill_attr: u8,
    exram_mode: u8,
    exram: [u8; 1024],
    // Multiplier
    multiplicand: u8,
    multiplier: u8,
    // IRQ scanline counter
    irq_scanline: u8,
    irq_enabled: bool,
    irq_pending: bool,
    in_frame: bool,
    #[allow(dead_code)]
    scanline_counter: u8,
    // Mirroring
    mirroring: Mirroring,
}

impl Mmc5 {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>) -> Self {
        let prg_ram = vec![0u8; 65536]; // 64K max SRAM
        Self {
            prg_rom, chr_rom, prg_ram,
            prg_mode: 3, prg_banks: [0, 0, 0, 0, 0xFF],
            prg_ram_protect_1: 0, prg_ram_protect_2: 0,
            chr_mode: 0, chr_banks: [0; 12], chr_upper: 0,
            last_chr_write_sprite: false,
            nametable_mapping: 0, fill_tile: 0, fill_attr: 0,
            exram_mode: 0, exram: [0; 1024],
            multiplicand: 0, multiplier: 0,
            irq_scanline: 0, irq_enabled: false, irq_pending: false,
            in_frame: false, scanline_counter: 0,
            mirroring: Mirroring::Vertical,
        }
    }
    fn prg_8k_count(&self) -> usize { self.prg_rom.len() / 8192 }
    fn read_prg_bank(&self, bank: u8, offset: usize) -> u8 {
        // Bit 7: 1 = ROM, 0 = RAM
        if bank & 0x80 != 0 {
            let rom_bank = (bank & 0x7F) as usize % self.prg_8k_count();
            self.prg_rom[rom_bank * 8192 + offset]
        } else {
            let ram_bank = (bank & 0x07) as usize;
            let ram_offset = ram_bank * 8192 + offset;
            if ram_offset < self.prg_ram.len() { self.prg_ram[ram_offset] } else { 0 }
        }
    }
    fn prg_ram_writable(&self) -> bool {
        self.prg_ram_protect_1 == 0x02 && self.prg_ram_protect_2 == 0x01
    }
}

impl Mapper for Mmc5 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x5204 => {
                let mut val = 0u8;
                if self.irq_pending { val |= 0x80; }
                if self.in_frame { val |= 0x40; }
                val
            }
            0x5205 => {
                let product = u16::from(self.multiplicand) * u16::from(self.multiplier);
                product as u8
            }
            0x5206 => {
                let product = u16::from(self.multiplicand) * u16::from(self.multiplier);
                (product >> 8) as u8
            }
            0x5C00..=0x5FFF => {
                if self.exram_mode >= 2 {
                    self.exram[(addr - 0x5C00) as usize]
                } else {
                    0
                }
            }
            0x6000..=0x7FFF => {
                let bank = self.prg_banks[0] & 0x7F;
                let ram_bank = (bank & 0x07) as usize;
                let offset = (addr - 0x6000) as usize;
                let ram_offset = ram_bank * 8192 + offset;
                if ram_offset < self.prg_ram.len() { self.prg_ram[ram_offset] } else { 0 }
            }
            0x8000..=0xFFFF => {
                let offset_in_32k = (addr - 0x8000) as usize;
                match self.prg_mode {
                    0 => {
                        // One 32K bank at $8000
                        let bank = (self.prg_banks[4] & 0x7C) | 0x80;
                        let sub = offset_in_32k / 8192;
                        self.read_prg_bank(bank.wrapping_add(sub as u8), offset_in_32k & 0x1FFF)
                    }
                    1 => {
                        // Two 16K banks
                        match addr {
                            0x8000..=0xBFFF => {
                                let bank = self.prg_banks[2] & 0xFE;
                                let sub = if addr < 0xA000 { 0 } else { 1 };
                                self.read_prg_bank(bank | sub, addr as usize & 0x1FFF)
                            }
                            _ => {
                                let bank = (self.prg_banks[4] & 0xFE) | 0x80;
                                let sub = if addr < 0xE000 { 0 } else { 1 };
                                self.read_prg_bank(bank | sub, addr as usize & 0x1FFF)
                            }
                        }
                    }
                    2 => {
                        // 16K + 8K + 8K
                        match addr {
                            0x8000..=0xBFFF => {
                                let bank = self.prg_banks[2] & 0xFE;
                                let sub = if addr < 0xA000 { 0 } else { 1 };
                                self.read_prg_bank(bank | sub, addr as usize & 0x1FFF)
                            }
                            0xC000..=0xDFFF => self.read_prg_bank(self.prg_banks[3], (addr - 0xC000) as usize),
                            _ => self.read_prg_bank(self.prg_banks[4] | 0x80, (addr - 0xE000) as usize),
                        }
                    }
                    3 | _ => {
                        // Four 8K banks
                        match addr {
                            0x8000..=0x9FFF => self.read_prg_bank(self.prg_banks[1], (addr - 0x8000) as usize),
                            0xA000..=0xBFFF => self.read_prg_bank(self.prg_banks[2], (addr - 0xA000) as usize),
                            0xC000..=0xDFFF => self.read_prg_bank(self.prg_banks[3], (addr - 0xC000) as usize),
                            _ => self.read_prg_bank(self.prg_banks[4] | 0x80, (addr - 0xE000) as usize),
                        }
                    }
                }
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x5100 => self.prg_mode = value & 0x03,
            0x5101 => self.chr_mode = value & 0x03,
            0x5102 => self.prg_ram_protect_1 = value & 0x03,
            0x5103 => self.prg_ram_protect_2 = value & 0x03,
            0x5104 => self.exram_mode = value & 0x03,
            0x5105 => {
                self.nametable_mapping = value;
                // Derive simple mirroring from bottom 2 bits
                let nt0 = value & 0x03;
                let nt1 = (value >> 2) & 0x03;
                if nt0 == nt1 {
                    self.mirroring = if nt0 & 1 == 0 { Mirroring::SingleScreenLower } else { Mirroring::SingleScreenUpper };
                } else if nt0 == 0 && nt1 == 1 {
                    self.mirroring = Mirroring::Vertical;
                } else {
                    self.mirroring = Mirroring::Horizontal;
                }
            }
            0x5106 => self.fill_tile = value,
            0x5107 => self.fill_attr = value & 0x03,
            0x5113 => self.prg_banks[0] = value & 0x07,
            0x5114 => self.prg_banks[1] = value,
            0x5115 => self.prg_banks[2] = value,
            0x5116 => self.prg_banks[3] = value,
            0x5117 => self.prg_banks[4] = value,
            0x5120..=0x5127 => {
                self.chr_banks[(addr - 0x5120) as usize] = u16::from(value) | (u16::from(self.chr_upper) << 8);
                self.last_chr_write_sprite = true;
            }
            0x5128..=0x512B => {
                self.chr_banks[8 + (addr - 0x5128) as usize] = u16::from(value) | (u16::from(self.chr_upper) << 8);
                self.last_chr_write_sprite = false;
            }
            0x5130 => self.chr_upper = value & 0x03,
            0x5203 => self.irq_scanline = value,
            0x5204 => {
                self.irq_enabled = value & 0x80 != 0;
            }
            0x5205 => self.multiplicand = value,
            0x5206 => self.multiplier = value,
            0x5C00..=0x5FFF => {
                if self.exram_mode < 2 {
                    self.exram[(addr - 0x5C00) as usize] = value;
                } else if self.exram_mode == 2 {
                    self.exram[(addr - 0x5C00) as usize] = value;
                }
            }
            0x6000..=0x7FFF => {
                if self.prg_ram_writable() {
                    let bank = (self.prg_banks[0] & 0x07) as usize;
                    let offset = (addr - 0x6000) as usize;
                    let ram_offset = bank * 8192 + offset;
                    if ram_offset < self.prg_ram.len() {
                        self.prg_ram[ram_offset] = value;
                    }
                }
            }
            0x8000..=0xDFFF => {
                // RAM banks can be written in modes 1-3
                if self.prg_ram_writable() {
                    let bank_reg = match (self.prg_mode, addr) {
                        (3, 0x8000..=0x9FFF) => Some(self.prg_banks[1]),
                        (3, 0xA000..=0xBFFF) | (2, 0x8000..=0xBFFF) => Some(self.prg_banks[2]),
                        (3, 0xC000..=0xDFFF) | (2, 0xC000..=0xDFFF) => Some(self.prg_banks[3]),
                        _ => None,
                    };
                    if let Some(bank) = bank_reg {
                        if bank & 0x80 == 0 {
                            let ram_bank = (bank & 0x07) as usize;
                            let offset = addr as usize & 0x1FFF;
                            let ram_offset = ram_bank * 8192 + offset;
                            if ram_offset < self.prg_ram.len() {
                                self.prg_ram[ram_offset] = value;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        if self.chr_rom.is_empty() { return 0; }
        let a = addr as usize & 0x1FFF;
        // Simplified: use sprite banks ($5120-$5127) for all CHR reads
        let bank = match self.chr_mode {
            0 => {
                // 8K mode: bank from $5127
                let base = self.chr_banks[7] as usize;
                base * 8192 + a
            }
            1 => {
                // 4K mode
                let half = a / 4096;
                let base = if half == 0 { self.chr_banks[3] } else { self.chr_banks[7] } as usize;
                base * 4096 + (a & 0x0FFF)
            }
            2 => {
                // 2K mode
                let slot = a / 2048;
                let base = self.chr_banks[match slot { 0 => 1, 1 => 3, 2 => 5, _ => 7 }] as usize;
                base * 2048 + (a & 0x7FF)
            }
            3 | _ => {
                // 1K mode
                let slot = a / 1024;
                let base = self.chr_banks[slot] as usize;
                base * 1024 + (a & 0x3FF)
            }
        };
        self.chr_rom[bank % self.chr_rom.len()]
    }

    fn chr_write(&mut self, _addr: u16, _value: u8) {}

    fn mirroring(&self) -> Mirroring { self.mirroring }
    fn irq_pending(&self) -> bool { self.irq_pending }

    fn prg_ram(&self) -> Option<&[u8]> { Some(&self.prg_ram) }
    fn set_prg_ram(&mut self, data: &[u8]) {
        let len = data.len().min(self.prg_ram.len());
        self.prg_ram[..len].copy_from_slice(&data[..len]);
    }
}

// ===== Remaining commercial mappers (trivial through medium) =====

/// CPROM (Mapper 13): 32K PRG + 4K CHR-RAM banking. Used by Videomation.
struct Cprom { prg_rom: Vec<u8>, chr_ram: [u8; 16384], chr_bank: u8, mirroring: Mirroring }
impl Cprom {
    fn new(prg_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self { prg_rom, chr_ram: [0; 16384], chr_bank: 0, mirroring }
    }
}
impl Mapper for Cprom {
    fn cpu_read(&self, addr: u16) -> u8 {
        if addr >= 0x8000 { self.prg_rom[(addr as usize - 0x8000) % self.prg_rom.len()] } else { 0 }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 { self.chr_bank = value & 0x03; }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        let a = addr as usize & 0x1FFF;
        if a < 0x1000 { self.chr_ram[a] } else { self.chr_ram[self.chr_bank as usize * 4096 + (a & 0xFFF)] }
    }
    fn chr_write(&mut self, addr: u16, value: u8) {
        let a = addr as usize & 0x1FFF;
        if a < 0x1000 { self.chr_ram[a] = value; } else { self.chr_ram[self.chr_bank as usize * 4096 + (a & 0xFFF)] = value; }
    }
    fn mirroring(&self) -> Mirroring { self.mirroring }
}

/// Bit Corp (Mapper 38): 32K PRG + 8K CHR via $7000-$7FFF.
struct BitCorp { prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring, prg_bank: u8, chr_bank: u8 }
impl BitCorp {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self { prg_rom, chr_rom, mirroring, prg_bank: 0, chr_bank: 0 }
    }
}
impl Mapper for BitCorp {
    fn cpu_read(&self, addr: u16) -> u8 {
        if addr >= 0x8000 { let idx = (self.prg_bank as usize * 32768 + (addr as usize - 0x8000)) % self.prg_rom.len(); self.prg_rom[idx] } else { 0 }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        if (0x7000..=0x7FFF).contains(&addr) { self.prg_bank = value & 0x03; self.chr_bank = (value >> 2) & 0x03; }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        if self.chr_rom.is_empty() { return 0; }
        let idx = (self.chr_bank as usize * 8192 + (addr as usize & 0x1FFF)) % self.chr_rom.len();
        self.chr_rom[idx]
    }
    fn chr_write(&mut self, _a: u16, _v: u8) {}
    fn mirroring(&self) -> Mirroring { self.mirroring }
}

/// MMC3 multicart wrapper: outer bank register selects game, inner uses MMC3.
/// Mapper 37 (PAL-ZZ: 3-in-1) and Mapper 47 (NES-QJ: 2-in-1).
#[allow(dead_code)]
struct Mmc3Multicart {
    inner: Mmc3,
    outer_bank: u8,
    outer_mask: u8,
    prg_outer_shift: u8,
    chr_outer_shift: u8,
}

impl Mmc3Multicart {
    fn new_47(prg_rom: Vec<u8>, chr_data: Vec<u8>) -> Self {
        Self {
            inner: Mmc3::new(prg_rom, chr_data),
            outer_bank: 0, outer_mask: 0x01, prg_outer_shift: 4, chr_outer_shift: 7,
        }
    }
    fn new_37(prg_rom: Vec<u8>, chr_data: Vec<u8>) -> Self {
        Self {
            inner: Mmc3::new(prg_rom, chr_data),
            outer_bank: 0, outer_mask: 0x07, prg_outer_shift: 4, chr_outer_shift: 7,
        }
    }
}

impl Mapper for Mmc3Multicart {
    fn cpu_read(&self, addr: u16) -> u8 { self.inner.cpu_read(addr) }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            self.outer_bank = value & self.outer_mask;
        } else {
            self.inner.cpu_write(addr, value);
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 { self.inner.chr_read(addr) }
    fn chr_write(&mut self, addr: u16, value: u8) { self.inner.chr_write(addr, value) }
    fn mirroring(&self) -> Mirroring { self.inner.mirroring() }
    fn irq_pending(&self) -> bool { self.inner.irq_pending() }
    fn prg_ram(&self) -> Option<&[u8]> { self.inner.prg_ram() }
    fn set_prg_ram(&mut self, data: &[u8]) { self.inner.set_prg_ram(data) }
}

/// Caltron 6-in-1 (Mapper 41): outer bank at $6000, inner at $8000.
struct Caltron {
    prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring,
    prg_bank: u8, chr_bank: u8,
}
impl Caltron {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self { prg_rom, chr_rom, mirroring, prg_bank: 0, chr_bank: 0 }
    }
}
impl Mapper for Caltron {
    fn cpu_read(&self, addr: u16) -> u8 {
        if addr >= 0x8000 { let idx = (self.prg_bank as usize * 32768 + (addr as usize - 0x8000)) % self.prg_rom.len(); self.prg_rom[idx] } else { 0 }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x67FF => {
                self.prg_bank = (addr & 0x07) as u8;
                self.mirroring = if addr & 0x08 != 0 { Mirroring::Horizontal } else { Mirroring::Vertical };
                // CHR high bits
                self.chr_bank = (self.chr_bank & 0x03) | (((addr as u8 >> 1) & 0x0C));
            }
            0x8000..=0xFFFF => {
                self.chr_bank = (self.chr_bank & 0x0C) | (value & 0x03);
            }
            _ => {}
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        if self.chr_rom.is_empty() { return 0; }
        let idx = (self.chr_bank as usize * 8192 + (addr as usize & 0x1FFF)) % self.chr_rom.len();
        self.chr_rom[idx]
    }
    fn chr_write(&mut self, _a: u16, _v: u8) {}
    fn mirroring(&self) -> Mirroring { self.mirroring }
}

/// RAMBO-1 / Tengen (Mapper 64): MMC3 variant with 3 PRG banks and 1K CHR mode.
struct Rambo1 {
    prg_rom: Vec<u8>, chr: Vec<u8>, chr_is_ram: bool, prg_ram: [u8; 8192],
    bank_select: u8, registers: [u8; 16], mirroring: Mirroring,
    irq_latch: u8, irq_counter: u8, irq_reload_flag: bool,
    irq_enabled: bool, irq_pending: bool, last_a12: bool,
}
impl Rambo1 {
    fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>) -> Self {
        let chr_is_ram = chr_data.is_empty();
        let chr = if chr_is_ram { vec![0u8; 8192] } else { chr_data };
        Self { prg_rom, chr, chr_is_ram, prg_ram: [0; 8192],
            bank_select: 0, registers: [0; 16], mirroring: Mirroring::Vertical,
            irq_latch: 0, irq_counter: 0, irq_reload_flag: false,
            irq_enabled: false, irq_pending: false, last_a12: false }
    }
    fn prg_8k_count(&self) -> usize { self.prg_rom.len() / 8192 }
    fn read_prg_8k(&self, bank: usize, offset: usize) -> u8 {
        self.prg_rom[(bank % self.prg_8k_count()) * 8192 + offset]
    }
}
impl Mapper for Rambo1 {
    fn cpu_read(&self, addr: u16) -> u8 {
        let last = self.prg_8k_count();
        let mode = self.bank_select & 0x40 != 0;
        match addr {
            0x6000..=0x7FFF => self.prg_ram[(addr - 0x6000) as usize],
            0x8000..=0x9FFF => {
                let bank = if mode { self.registers[0xF] as usize } else { self.registers[6] as usize };
                self.read_prg_8k(bank, (addr - 0x8000) as usize)
            }
            0xA000..=0xBFFF => self.read_prg_8k(self.registers[7] as usize, (addr - 0xA000) as usize),
            0xC000..=0xDFFF => {
                let bank = if mode { self.registers[6] as usize } else { self.registers[0xF] as usize };
                self.read_prg_8k(bank, (addr - 0xC000) as usize)
            }
            0xE000..=0xFFFF => self.read_prg_8k(last - 1, (addr - 0xE000) as usize),
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => self.prg_ram[(addr - 0x6000) as usize] = value,
            0x8000..=0x9FFE if addr & 1 == 0 => self.bank_select = value,
            0x8001..=0x9FFF if addr & 1 == 1 => {
                let reg = (self.bank_select & 0x0F) as usize;
                if reg < 16 { self.registers[reg] = value; }
            }
            0xA000..=0xBFFE if addr & 1 == 0 => {
                self.mirroring = if value & 1 != 0 { Mirroring::Horizontal } else { Mirroring::Vertical };
            }
            0xC000..=0xDFFE if addr & 1 == 0 => self.irq_latch = value,
            0xC001..=0xDFFF if addr & 1 == 1 => self.irq_reload_flag = true,
            0xE000..=0xFFFE if addr & 1 == 0 => { self.irq_enabled = false; self.irq_pending = false; }
            0xE001..=0xFFFF if addr & 1 == 1 => self.irq_enabled = true,
            _ => {}
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        let a = addr as usize & 0x1FFF;
        let a12 = a >= 0x1000;
        if a12 && !self.last_a12 {
            if self.irq_counter == 0 || self.irq_reload_flag { self.irq_counter = self.irq_latch; self.irq_reload_flag = false; }
            else { self.irq_counter -= 1; }
            if self.irq_counter == 0 && self.irq_enabled { self.irq_pending = true; }
        }
        self.last_a12 = a12;
        let chr_inv = self.bank_select & 0x80 != 0;
        let k1_mode = self.bank_select & 0x20 != 0;
        let base = if chr_inv { 0x1000 } else { 0 };
        let e = a ^ base;
        let bank = if k1_mode {
            // 1K mode: R0-R5 + R8,R9 for all 8 slots
            match e {
                0x0000..=0x03FF => self.registers[0] as usize,
                0x0400..=0x07FF => self.registers[8] as usize,
                0x0800..=0x0BFF => self.registers[1] as usize,
                0x0C00..=0x0FFF => self.registers[9] as usize,
                0x1000..=0x13FF => self.registers[2] as usize,
                0x1400..=0x17FF => self.registers[3] as usize,
                0x1800..=0x1BFF => self.registers[4] as usize,
                _ => self.registers[5] as usize,
            }
        } else {
            // 2K/1K mode (standard MMC3-like)
            match e {
                0x0000..=0x03FF => self.registers[0] as usize & 0xFE,
                0x0400..=0x07FF => self.registers[0] as usize | 1,
                0x0800..=0x0BFF => self.registers[1] as usize & 0xFE,
                0x0C00..=0x0FFF => self.registers[1] as usize | 1,
                0x1000..=0x13FF => self.registers[2] as usize,
                0x1400..=0x17FF => self.registers[3] as usize,
                0x1800..=0x1BFF => self.registers[4] as usize,
                _ => self.registers[5] as usize,
            }
        };
        let offset = a & 0x3FF;
        let idx = (bank * 1024 + offset) % self.chr.len();
        self.chr[idx]
    }
    fn chr_write(&mut self, addr: u16, value: u8) {
        if self.chr_is_ram { self.chr[(addr as usize) & 0x1FFF] = value; }
    }
    fn mirroring(&self) -> Mirroring { self.mirroring }
    fn irq_pending(&self) -> bool { self.irq_pending }
    fn prg_ram(&self) -> Option<&[u8]> { Some(&self.prg_ram) }
    fn set_prg_ram(&mut self, data: &[u8]) { let l = data.len().min(8192); self.prg_ram[..l].copy_from_slice(&data[..l]); }
}

/// VRC3 (Mapper 73): PRG banking + 16-bit IRQ. Used by Salamander.
struct Vrc3 {
    prg_rom: Vec<u8>, chr_ram: [u8; 8192], mirroring: Mirroring,
    prg_bank: u8, irq_latch: u16, irq_counter: u16,
    irq_enabled: bool, irq_pending: bool, irq_mode_8bit: bool,
}
impl Vrc3 {
    fn new(prg_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self { prg_rom, chr_ram: [0; 8192], mirroring, prg_bank: 0,
            irq_latch: 0, irq_counter: 0, irq_enabled: false, irq_pending: false, irq_mode_8bit: false }
    }
    fn prg_16k_count(&self) -> usize { self.prg_rom.len() / 16384 }
}
impl Mapper for Vrc3 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xBFFF => { let b = self.prg_bank as usize % self.prg_16k_count(); self.prg_rom[b * 16384 + (addr - 0x8000) as usize] }
            0xC000..=0xFFFF => { let b = self.prg_16k_count() - 1; self.prg_rom[b * 16384 + (addr - 0xC000) as usize] }
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr & 0xF000 {
            0x8000 => self.irq_latch = (self.irq_latch & 0xFFF0) | u16::from(value & 0x0F),
            0x9000 => self.irq_latch = (self.irq_latch & 0xFF0F) | (u16::from(value & 0x0F) << 4),
            0xA000 => self.irq_latch = (self.irq_latch & 0xF0FF) | (u16::from(value & 0x0F) << 8),
            0xB000 => self.irq_latch = (self.irq_latch & 0x0FFF) | (u16::from(value & 0x0F) << 12),
            0xC000 => {
                self.irq_pending = false;
                self.irq_enabled = value & 0x02 != 0;
                self.irq_mode_8bit = value & 0x04 != 0;
                if self.irq_enabled { self.irq_counter = self.irq_latch; }
            }
            0xD000 => { self.irq_pending = false; self.irq_enabled = value & 0x02 != 0; }
            0xF000 => self.prg_bank = value & 0x07,
            _ => {}
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 { self.chr_ram[(addr as usize) & 0x1FFF] }
    fn chr_write(&mut self, addr: u16, value: u8) { self.chr_ram[(addr as usize) & 0x1FFF] = value; }
    fn mirroring(&self) -> Mirroring { self.mirroring }
    fn irq_pending(&self) -> bool { self.irq_pending }
}

/// Namcot-3446/76 (Mapper 76): 8K PRG + 2K CHR. Used by Megami Tensei.
struct Namcot3446v2 {
    prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring,
    bank_select: u8, prg_banks: [u8; 2], chr_banks: [u8; 4],
}
impl Namcot3446v2 {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self { prg_rom, chr_rom, mirroring, bank_select: 0, prg_banks: [0; 2], chr_banks: [0; 4] }
    }
    fn prg_8k_count(&self) -> usize { self.prg_rom.len() / 8192 }
}
impl Mapper for Namcot3446v2 {
    fn cpu_read(&self, addr: u16) -> u8 {
        let c = self.prg_8k_count();
        match addr {
            0x8000..=0x9FFF => self.prg_rom[(self.prg_banks[0] as usize % c) * 8192 + (addr - 0x8000) as usize],
            0xA000..=0xBFFF => self.prg_rom[(self.prg_banks[1] as usize % c) * 8192 + (addr - 0xA000) as usize],
            0xC000..=0xDFFF => self.prg_rom[(c - 2) * 8192 + (addr - 0xC000) as usize],
            0xE000..=0xFFFF => self.prg_rom[(c - 1) * 8192 + (addr - 0xE000) as usize],
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x8000 => self.bank_select = value & 0x07,
            0x8001 => match self.bank_select {
                2 => self.chr_banks[0] = value & 0x3F,
                3 => self.chr_banks[1] = value & 0x3F,
                4 => self.chr_banks[2] = value & 0x3F,
                5 => self.chr_banks[3] = value & 0x3F,
                6 => self.prg_banks[0] = value & 0x3F,
                7 => self.prg_banks[1] = value & 0x3F,
                _ => {}
            }
            _ => {}
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        if self.chr_rom.is_empty() { return 0; }
        let a = addr as usize & 0x1FFF;
        let bank = self.chr_banks[a / 2048] as usize;
        let idx = (bank * 2048 + (a & 0x7FF)) % self.chr_rom.len();
        self.chr_rom[idx]
    }
    fn chr_write(&mut self, _a: u16, _v: u8) {}
    fn mirroring(&self) -> Mirroring { self.mirroring }
}

/// Jaleco JF-13 (Mapper 86): 32K PRG + 8K CHR. Used by Moero!! Pro Yakyuu.
struct JalecoJf13 { prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring, prg_bank: u8, chr_bank: u8 }
impl JalecoJf13 {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self { prg_rom, chr_rom, mirroring, prg_bank: 0, chr_bank: 0 }
    }
}
impl Mapper for JalecoJf13 {
    fn cpu_read(&self, addr: u16) -> u8 {
        if addr >= 0x8000 { let idx = (self.prg_bank as usize * 32768 + (addr as usize - 0x8000)) % self.prg_rom.len(); self.prg_rom[idx] } else { 0 }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x6FFF).contains(&addr) {
            self.prg_bank = (value >> 4) & 0x03;
            self.chr_bank = (value & 0x03) | ((value >> 4) & 0x04);
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        if self.chr_rom.is_empty() { return 0; }
        let idx = (self.chr_bank as usize * 8192 + (addr as usize & 0x1FFF)) % self.chr_rom.len();
        self.chr_rom[idx]
    }
    fn chr_write(&mut self, _a: u16, _v: u8) {}
    fn mirroring(&self) -> Mirroring { self.mirroring }
}

/// Sunsoft-2 variant (Mapper 89): 16K PRG + 8K CHR + single-screen mirroring.
struct Sunsoft2b { prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring, prg_bank: u8, chr_bank: u8 }
impl Sunsoft2b {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>) -> Self {
        Self { prg_rom, chr_rom, mirroring: Mirroring::SingleScreenLower, prg_bank: 0, chr_bank: 0 }
    }
    fn prg_16k_count(&self) -> usize { self.prg_rom.len() / 16384 }
}
impl Mapper for Sunsoft2b {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xBFFF => { let b = self.prg_bank as usize % self.prg_16k_count(); self.prg_rom[b * 16384 + (addr - 0x8000) as usize] }
            0xC000..=0xFFFF => { let b = self.prg_16k_count() - 1; self.prg_rom[b * 16384 + (addr - 0xC000) as usize] }
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            self.prg_bank = (value >> 4) & 0x07;
            self.chr_bank = (value & 0x07) | ((value >> 4) & 0x08);
            self.mirroring = if value & 0x08 != 0 { Mirroring::SingleScreenUpper } else { Mirroring::SingleScreenLower };
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        if self.chr_rom.is_empty() { return 0; }
        let idx = (self.chr_bank as usize * 8192 + (addr as usize & 0x1FFF)) % self.chr_rom.len();
        self.chr_rom[idx]
    }
    fn chr_write(&mut self, _a: u16, _v: u8) {}
    fn mirroring(&self) -> Mirroring { self.mirroring }
}

/// Jaleco JF-19 (Mapper 92): like mapper 72 but $C000 switchable, $8000 fixed.
struct JalecoJf19 { prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring, prg_bank: u8, chr_bank: u8 }
impl JalecoJf19 {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self { prg_rom, chr_rom, mirroring, prg_bank: 0, chr_bank: 0 }
    }
    fn prg_16k_count(&self) -> usize { self.prg_rom.len() / 16384 }
}
impl Mapper for JalecoJf19 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xBFFF => self.prg_rom[(addr - 0x8000) as usize % (self.prg_16k_count() * 16384).min(self.prg_rom.len())],
            0xC000..=0xFFFF => { let b = self.prg_bank as usize % self.prg_16k_count(); self.prg_rom[b * 16384 + (addr - 0xC000) as usize] }
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            if value & 0x80 != 0 { self.prg_bank = value & 0x0F; }
            if value & 0x40 != 0 { self.chr_bank = value & 0x0F; }
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        if self.chr_rom.is_empty() { return 0; }
        let idx = (self.chr_bank as usize * 8192 + (addr as usize & 0x1FFF)) % self.chr_rom.len();
        self.chr_rom[idx]
    }
    fn chr_write(&mut self, _a: u16, _v: u8) {}
    fn mirroring(&self) -> Mirroring { self.mirroring }
}

/// UN1ROM (Mapper 94): UNROM variant with bank bits at D2-D4. Used by Senjou no Ookami.
struct Un1rom { prg_rom: Vec<u8>, chr_ram: [u8; 8192], mirroring: Mirroring, prg_bank: u8 }
impl Un1rom {
    fn new(prg_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self { prg_rom, chr_ram: [0; 8192], mirroring, prg_bank: 0 }
    }
    fn prg_16k_count(&self) -> usize { self.prg_rom.len() / 16384 }
}
impl Mapper for Un1rom {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xBFFF => { let b = self.prg_bank as usize % self.prg_16k_count(); self.prg_rom[b * 16384 + (addr - 0x8000) as usize] }
            0xC000..=0xFFFF => { let b = self.prg_16k_count() - 1; self.prg_rom[b * 16384 + (addr - 0xC000) as usize] }
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 { self.prg_bank = (value >> 2) & 0x07; }
    }
    fn chr_read(&mut self, addr: u16) -> u8 { self.chr_ram[(addr as usize) & 0x1FFF] }
    fn chr_write(&mut self, addr: u16, value: u8) { self.chr_ram[(addr as usize) & 0x1FFF] = value; }
    fn mirroring(&self) -> Mirroring { self.mirroring }
}

/// Namcot-3425 (Mapper 95): like mapper 206 but D5 of CHR controls nametable.
struct Namcot3425 {
    prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring,
    bank_select: u8, registers: [u8; 8],
}
impl Namcot3425 {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self { prg_rom, chr_rom, mirroring, bank_select: 0, registers: [0; 8] }
    }
    fn prg_8k_count(&self) -> usize { self.prg_rom.len() / 8192 }
}
impl Mapper for Namcot3425 {
    fn cpu_read(&self, addr: u16) -> u8 {
        let c = self.prg_8k_count();
        match addr {
            0x8000..=0x9FFF => self.prg_rom[(self.registers[6] as usize % c) * 8192 + (addr - 0x8000) as usize],
            0xA000..=0xBFFF => self.prg_rom[(self.registers[7] as usize % c) * 8192 + (addr - 0xA000) as usize],
            0xC000..=0xDFFF => self.prg_rom[(c - 2) * 8192 + (addr - 0xC000) as usize],
            0xE000..=0xFFFF => self.prg_rom[(c - 1) * 8192 + (addr - 0xE000) as usize],
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x8000 => self.bank_select = value & 0x07,
            0x8001 => {
                let reg = self.bank_select as usize;
                if reg < 8 { self.registers[reg] = value; }
                // D5 of CHR registers 0/1 controls nametable mirroring
                if reg == 0 || reg == 1 {
                    self.mirroring = if value & 0x20 != 0 { Mirroring::SingleScreenUpper } else { Mirroring::SingleScreenLower };
                }
            }
            _ => {}
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        if self.chr_rom.is_empty() { return 0; }
        let a = addr as usize & 0x1FFF;
        let (bank, offset) = match a {
            0x0000..=0x07FF => ((self.registers[0] as usize & 0x1F) & 0xFE, a),
            0x0800..=0x0FFF => ((self.registers[1] as usize & 0x1F) & 0xFE, a & 0x7FF),
            0x1000..=0x13FF => (self.registers[2] as usize & 0x3F, a & 0x3FF),
            0x1400..=0x17FF => (self.registers[3] as usize & 0x3F, a & 0x3FF),
            0x1800..=0x1BFF => (self.registers[4] as usize & 0x3F, a & 0x3FF),
            _ => (self.registers[5] as usize & 0x3F, a & 0x3FF),
        };
        let idx = (bank * 1024 + offset) % self.chr_rom.len();
        self.chr_rom[idx]
    }
    fn chr_write(&mut self, _a: u16, _v: u8) {}
    fn mirroring(&self) -> Mirroring { self.mirroring }
}

/// Irem TAM-S1 (Mapper 97): 16K PRG at $C000 (switchable), $8000 fixed to last.
struct IremTamS1 { prg_rom: Vec<u8>, chr_ram: [u8; 8192], mirroring: Mirroring, prg_bank: u8 }
impl IremTamS1 {
    fn new(prg_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self { prg_rom, chr_ram: [0; 8192], mirroring, prg_bank: 0 }
    }
    fn prg_16k_count(&self) -> usize { self.prg_rom.len() / 16384 }
}
impl Mapper for IremTamS1 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xBFFF => { let b = self.prg_16k_count() - 1; self.prg_rom[b * 16384 + (addr - 0x8000) as usize] }
            0xC000..=0xFFFF => { let b = self.prg_bank as usize % self.prg_16k_count(); self.prg_rom[b * 16384 + (addr - 0xC000) as usize] }
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            self.prg_bank = value & 0x0F;
            self.mirroring = match (value >> 6) & 0x03 {
                0 => Mirroring::SingleScreenLower,
                1 => Mirroring::SingleScreenUpper,
                2 => Mirroring::Vertical,
                _ => Mirroring::Horizontal,
            };
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 { self.chr_ram[(addr as usize) & 0x1FFF] }
    fn chr_write(&mut self, addr: u16, value: u8) { self.chr_ram[(addr as usize) & 0x1FFF] = value; }
    fn mirroring(&self) -> Mirroring { self.mirroring }
}

/// Camerica Quattro (Mapper 232): outer+inner PRG banking. Quattro multicarts.
struct CamericaQuattro { prg_rom: Vec<u8>, chr_ram: [u8; 8192], mirroring: Mirroring, outer: u8, inner: u8 }
impl CamericaQuattro {
    fn new(prg_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self { prg_rom, chr_ram: [0; 8192], mirroring, outer: 0, inner: 3 }
    }
    fn prg_16k_count(&self) -> usize { self.prg_rom.len() / 16384 }
}
impl Mapper for CamericaQuattro {
    fn cpu_read(&self, addr: u16) -> u8 {
        let c = self.prg_16k_count();
        let base = (self.outer as usize & 0x18) >> 1; // Outer selects 64K block
        match addr {
            0x8000..=0xBFFF => { let b = (base | self.inner as usize & 0x03) % c; self.prg_rom[b * 16384 + (addr - 0x8000) as usize] }
            0xC000..=0xFFFF => { let b = (base | 0x03) % c; self.prg_rom[b * 16384 + (addr - 0xC000) as usize] }
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x8000..=0xBFFF => self.outer = value,
            0xC000..=0xFFFF => self.inner = value,
            _ => {}
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 { self.chr_ram[(addr as usize) & 0x1FFF] }
    fn chr_write(&mut self, addr: u16, value: u8) { self.chr_ram[(addr as usize) & 0x1FFF] = value; }
    fn mirroring(&self) -> Mirroring { self.mirroring }
}

/// HES NTD-8 (Mapper 113): NINA-003 with extra bits. HES multicarts.
struct HesNtd8 { prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring, prg_bank: u8, chr_bank: u8 }
impl HesNtd8 {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self { prg_rom, chr_rom, mirroring, prg_bank: 0, chr_bank: 0 }
    }
}
impl Mapper for HesNtd8 {
    fn cpu_read(&self, addr: u16) -> u8 {
        if addr >= 0x8000 { let idx = (self.prg_bank as usize * 32768 + (addr as usize - 0x8000)) % self.prg_rom.len(); self.prg_rom[idx] } else { 0 }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        if (0x4100..=0x5FFF).contains(&addr) {
            self.prg_bank = (value >> 3) & 0x07;
            self.chr_bank = (value & 0x07) | ((value >> 3) & 0x08);
            self.mirroring = if value & 0x80 != 0 { Mirroring::Vertical } else { Mirroring::Horizontal };
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        if self.chr_rom.is_empty() { return 0; }
        let idx = (self.chr_bank as usize * 8192 + (addr as usize & 0x1FFF)) % self.chr_rom.len();
        self.chr_rom[idx]
    }
    fn chr_write(&mut self, _a: u16, _v: u8) {}
    fn mirroring(&self) -> Mirroring { self.mirroring }
}

/// Bandai SRAM variant (Mapper 153): mapper 16 with SRAM instead of EEPROM.
/// Implemented as alias to BandaiFcg.

/// Namcot-3453 (Mapper 154): like 206 but D6 controls mirroring.
struct Namcot3453 {
    prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring,
    bank_select: u8, registers: [u8; 8],
}
impl Namcot3453 {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self { prg_rom, chr_rom, mirroring, bank_select: 0, registers: [0; 8] }
    }
    fn prg_8k_count(&self) -> usize { self.prg_rom.len() / 8192 }
}
impl Mapper for Namcot3453 {
    fn cpu_read(&self, addr: u16) -> u8 {
        let c = self.prg_8k_count();
        match addr {
            0x8000..=0x9FFF => self.prg_rom[(self.registers[6] as usize & 0x0F % c) * 8192 + (addr - 0x8000) as usize],
            0xA000..=0xBFFF => self.prg_rom[(self.registers[7] as usize & 0x0F % c) * 8192 + (addr - 0xA000) as usize],
            0xC000..=0xDFFF => self.prg_rom[(c - 2) * 8192 + (addr - 0xC000) as usize],
            0xE000..=0xFFFF => self.prg_rom[(c - 1) * 8192 + (addr - 0xE000) as usize],
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x8000 => {
                self.bank_select = value & 0x07;
                self.mirroring = if value & 0x40 != 0 { Mirroring::SingleScreenUpper } else { Mirroring::SingleScreenLower };
            }
            0x8001 => { let r = self.bank_select as usize; if r < 8 { self.registers[r] = value; } }
            _ => {}
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        if self.chr_rom.is_empty() { return 0; }
        let a = addr as usize & 0x1FFF;
        let (bank, offset) = match a {
            0x0000..=0x07FF => (self.registers[0] as usize & 0x3E, a),
            0x0800..=0x0FFF => (self.registers[1] as usize & 0x3E, a & 0x7FF),
            0x1000..=0x13FF => (self.registers[2] as usize & 0x3F, a & 0x3FF),
            0x1400..=0x17FF => (self.registers[3] as usize & 0x3F, a & 0x3FF),
            0x1800..=0x1BFF => (self.registers[4] as usize & 0x3F, a & 0x3FF),
            _ => (self.registers[5] as usize & 0x3F, a & 0x3FF),
        };
        let idx = (bank * 1024 + offset) % self.chr_rom.len();
        self.chr_rom[idx]
    }
    fn chr_write(&mut self, _a: u16, _v: u8) {}
    fn mirroring(&self) -> Mirroring { self.mirroring }
}

/// DAOU-306 (Mapper 156): per-byte CHR banking. Korean commercial games.
struct Daou306 {
    prg_rom: Vec<u8>, chr_rom: Vec<u8>, prg_bank: u8,
    chr_banks: [u8; 8],
}
impl Daou306 {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>) -> Self {
        Self { prg_rom, chr_rom, prg_bank: 0, chr_banks: [0; 8] }
    }
    fn prg_16k_count(&self) -> usize { self.prg_rom.len() / 16384 }
}
impl Mapper for Daou306 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xBFFF => { let b = self.prg_bank as usize % self.prg_16k_count(); self.prg_rom[b * 16384 + (addr - 0x8000) as usize] }
            0xC000..=0xFFFF => { let b = self.prg_16k_count() - 1; self.prg_rom[b * 16384 + (addr - 0xC000) as usize] }
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0xC000..=0xC007 => self.chr_banks[(addr - 0xC000) as usize] = value,
            0xC010 => self.prg_bank = value,
            _ => {}
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        if self.chr_rom.is_empty() { return 0; }
        let a = addr as usize & 0x1FFF;
        let bank = self.chr_banks[a / 1024] as usize;
        let idx = (bank * 1024 + (a & 0x3FF)) % self.chr_rom.len();
        self.chr_rom[idx]
    }
    fn chr_write(&mut self, _a: u16, _v: u8) {}
    fn mirroring(&self) -> Mirroring { Mirroring::SingleScreenLower }
}

/// Crazy Climber (Mapper 180): UNROM with fixed $8000, switchable $C000.
struct CrazyClimber { prg_rom: Vec<u8>, chr_ram: [u8; 8192], mirroring: Mirroring, prg_bank: u8 }
impl CrazyClimber {
    fn new(prg_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self { prg_rom, chr_ram: [0; 8192], mirroring, prg_bank: 0 }
    }
    fn prg_16k_count(&self) -> usize { self.prg_rom.len() / 16384 }
}
impl Mapper for CrazyClimber {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xBFFF => self.prg_rom[(addr - 0x8000) as usize % self.prg_rom.len()],
            0xC000..=0xFFFF => { let b = self.prg_bank as usize % self.prg_16k_count(); self.prg_rom[b * 16384 + (addr - 0xC000) as usize] }
            _ => 0,
        }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 { self.prg_bank = value & 0x07; }
    }
    fn chr_read(&mut self, addr: u16) -> u8 { self.chr_ram[(addr as usize) & 0x1FFF] }
    fn chr_write(&mut self, addr: u16, value: u8) { self.chr_ram[(addr as usize) & 0x1FFF] = value; }
    fn mirroring(&self) -> Mirroring { self.mirroring }
}

/// Action 52 (Mapper 228): PRG/CHR banking via address lines.
struct Action52 { prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring, prg_bank: u16, chr_bank: u8, prg_chip: u8 }
impl Action52 {
    fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: Mirroring) -> Self {
        Self { prg_rom, chr_rom, mirroring, prg_bank: 0, chr_bank: 0, prg_chip: 0 }
    }
}
impl Mapper for Action52 {
    fn cpu_read(&self, addr: u16) -> u8 {
        if addr >= 0x8000 {
            let offset = (addr as usize - 0x8000) % self.prg_rom.len();
            let bank_offset = self.prg_bank as usize * 16384;
            self.prg_rom[(bank_offset + (offset & 0x3FFF)) % self.prg_rom.len()]
        } else { 0 }
    }
    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            self.chr_bank = (value & 0x03) | (((addr & 0x0F) as u8) << 2);
            self.prg_bank = ((addr >> 6) & 0x1F) as u16;
            self.prg_chip = ((addr >> 11) & 0x03) as u8;
            self.mirroring = if addr & 0x2000 != 0 { Mirroring::Horizontal } else { Mirroring::Vertical };
        }
    }
    fn chr_read(&mut self, addr: u16) -> u8 {
        if self.chr_rom.is_empty() { return 0; }
        let idx = (self.chr_bank as usize * 8192 + (addr as usize & 0x1FFF)) % self.chr_rom.len();
        self.chr_rom[idx]
    }
    fn chr_write(&mut self, _a: u16, _v: u8) {}
    fn mirroring(&self) -> Mirroring { self.mirroring }
}

/// Parsed cartridge: mapper implementation and header metadata.
pub struct ParsedCartridge {
    pub mapper: Box<dyn Mapper>,
    pub has_battery: bool,
}

/// Parse an iNES file and return a parsed cartridge (mapper + metadata).
///
/// # Errors
///
/// Returns an error string if the header is invalid or the mapper is unsupported.
pub fn parse_ines(data: &[u8]) -> Result<ParsedCartridge, String> {
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

    // NES 2.0 detection: bits 3-2 of flags7 == 0b10
    let is_nes_2_0 = (flags7 & 0x0C) == 0x08;

    let (mapper_number, prg_size, chr_size) = if is_nes_2_0 {
        // NES 2.0: 12-bit mapper number
        let mapper8 = data[8];
        let mapper_number =
            u16::from(mapper_lo) | u16::from(mapper_hi) | (u16::from(mapper8 & 0x0F) << 8);

        // NES 2.0 extended PRG size: byte 9 low nibble << 8 | byte 4
        let prg_hi = usize::from(data[9] & 0x0F);
        let prg_size = (prg_hi << 8 | usize::from(prg_banks)) * 16384;

        // NES 2.0 extended CHR size: byte 9 high nibble << 8 | byte 5
        let chr_hi = usize::from((data[9] >> 4) & 0x0F);
        let chr_size = (chr_hi << 8 | usize::from(chr_banks)) * 8192;

        (mapper_number, prg_size, chr_size)
    } else {
        // iNES 1.0: 8-bit mapper number
        let mapper_number = u16::from(mapper_hi | mapper_lo);
        let prg_size = usize::from(prg_banks) * 16384;
        let chr_size = usize::from(chr_banks) * 8192;
        (mapper_number, prg_size, chr_size)
    };

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

    let mapper: Box<dyn Mapper> = match header.mapper_number {
        0 => Box::new(Nrom::new(prg_rom, chr_data, mirroring)),
        5 => Box::new(Mmc5::new(prg_rom, chr_data)),
        13 => Box::new(Cprom::new(prg_rom, mirroring)),
        1 => Box::new(Mmc1::new(prg_rom, chr_data)),
        2 => Box::new(UxRom::new(prg_rom, chr_data, mirroring)),
        3 => Box::new(CnRom::new(prg_rom, chr_data, mirroring)),
        4 => Box::new(Mmc3::new(prg_rom, chr_data)),
        7 => Box::new(AxRom::new(prg_rom)),
        9 => Box::new(Mmc2::new(prg_rom, chr_data)),
        10 => Box::new(Mmc4::new(prg_rom, chr_data)),
        11 => Box::new(ColorDreams::new(prg_rom, chr_data, mirroring)),
        34 => Box::new(BxRom::new(prg_rom, mirroring)),
        66 => Box::new(GxRom::new(prg_rom, chr_data, mirroring)),
        71 => Box::new(Camerica::new(prg_rom, mirroring)),
        87 => Box::new(Mapper87::new(prg_rom, chr_data, mirroring)),
        206 => Box::new(Mapper206::new(prg_rom, chr_data, mirroring)),
        16 | 159 => Box::new(BandaiFcg::new(prg_rom, chr_data, mirroring)),
        18 => Box::new(JalecoSs88006::new(prg_rom, chr_data, mirroring)),
        24 => Box::new(Vrc6::new(prg_rom, chr_data, mirroring, false)),
        26 => Box::new(Vrc6::new(prg_rom, chr_data, mirroring, true)),
        19 => Box::new(Namco163::new(prg_rom, chr_data, mirroring)),
        // Konami VRC2/VRC4 family — address line wiring varies by mapper number
        21 => Box::new(Vrc2Vrc4::new(prg_rom, chr_data, mirroring, 1, 2, false)),
        22 => Box::new(Vrc2Vrc4::new(prg_rom, chr_data, mirroring, 0, 1, true)),
        23 => Box::new(Vrc2Vrc4::new(prg_rom, chr_data, mirroring, 0, 1, false)),
        25 => Box::new(Vrc2Vrc4::new(prg_rom, chr_data, mirroring, 1, 0, false)),
        32 => Box::new(IremG101::new(prg_rom, chr_data, mirroring)),
        37 => Box::new(Mmc3Multicart::new_37(prg_rom, chr_data)),
        38 => Box::new(BitCorp::new(prg_rom, chr_data, mirroring)),
        41 => Box::new(Caltron::new(prg_rom, chr_data, mirroring)),
        47 => Box::new(Mmc3Multicart::new_47(prg_rom, chr_data)),
        33 => Box::new(TaitoTc0190::new(prg_rom, chr_data, mirroring)),
        64 => Box::new(Rambo1::new(prg_rom, chr_data)),
        65 => Box::new(IremH3001::new(prg_rom, chr_data, mirroring)),
        48 => Box::new(TaitoTc0690::new(prg_rom, chr_data, mirroring)),
        67 => Box::new(Sunsoft3::new(prg_rom, chr_data, mirroring)),
        68 => Box::new(Sunsoft4::new(prg_rom, chr_data, mirroring)),
        69 => Box::new(SunsoftFme7::new(prg_rom, chr_data, mirroring)),
        70 => Box::new(Bandai74161::new(prg_rom, chr_data, mirroring)),
        72 => Box::new(JalecoJf17::new(prg_rom, chr_data, mirroring)),
        73 => Box::new(Vrc3::new(prg_rom, mirroring)),
        75 => Box::new(Vrc1::new(prg_rom, chr_data, mirroring)),
        76 => Box::new(Namcot3446v2::new(prg_rom, chr_data, mirroring)),
        78 => Box::new(Irem74161::new(prg_rom, chr_data, mirroring, true)),
        79 => Box::new(Nina003::new(prg_rom, chr_data, mirroring)),
        80 => Box::new(TaitoX1005::new(prg_rom, chr_data, mirroring)),
        82 => Box::new(TaitoX1017::new(prg_rom, chr_data, mirroring)),
        85 => Box::new(Vrc7::new(prg_rom, chr_data, mirroring)),
        86 => Box::new(JalecoJf13::new(prg_rom, chr_data, mirroring)),
        89 => Box::new(Sunsoft2b::new(prg_rom, chr_data)),
        92 => Box::new(JalecoJf19::new(prg_rom, chr_data, mirroring)),
        94 => Box::new(Un1rom::new(prg_rom, mirroring)),
        95 => Box::new(Namcot3425::new(prg_rom, chr_data, mirroring)),
        97 => Box::new(IremTamS1::new(prg_rom, mirroring)),
        113 => Box::new(HesNtd8::new(prg_rom, chr_data, mirroring)),
        88 => Box::new(Namco3446::new(prg_rom, chr_data, mirroring)),
        118 => Box::new(TxSrom::new(prg_rom, chr_data)),
        119 => Box::new(Tqrom::new(prg_rom, chr_data)),
        93 => Box::new(Sunsoft2::new(prg_rom, mirroring)),
        140 => Box::new(JalecoJf11::new(prg_rom, chr_data, mirroring)),
        144 => Box::new(ColorDreams::new(prg_rom, chr_data, mirroring)), // Functionally identical to 11
        152 => Box::new(Bandai74161Ss::new(prg_rom, chr_data)),
        153 => Box::new(BandaiFcg::new(prg_rom, chr_data, mirroring)), // SRAM variant of 16
        154 => Box::new(Namcot3453::new(prg_rom, chr_data, mirroring)),
        156 => Box::new(Daou306::new(prg_rom, chr_data)),
        180 => Box::new(CrazyClimber::new(prg_rom, mirroring)),
        184 => Box::new(Sunsoft1::new(prg_rom, chr_data, mirroring)),
        185 => Box::new(CnromProtected::new(prg_rom, chr_data, mirroring)),
        210 => Box::new(Namco175::new(prg_rom, chr_data, mirroring)),
        228 => Box::new(Action52::new(prg_rom, chr_data, mirroring)),
        232 => Box::new(CamericaQuattro::new(prg_rom, mirroring)),
        n => return Err(format!("Unsupported mapper: {n}")),
    };

    Ok(ParsedCartridge {
        mapper,
        has_battery,
    })
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
        let mapper = parse_ines(&data).expect("parse failed").mapper;
        assert_eq!(mapper.mirroring(), Mirroring::Horizontal);
        // PRG at $8000 should be byte 0 of PRG ROM
        assert_eq!(mapper.cpu_read(0x8000), 0x00);
        // 16K mirrored: $C000 should mirror $8000
        assert_eq!(mapper.cpu_read(0xC000), 0x00);
    }

    #[test]
    fn parse_valid_nrom_32k() {
        let data = make_ines(2, 1, 0x01); // Vertical mirroring
        let mapper = parse_ines(&data).expect("parse failed").mapper;
        assert_eq!(mapper.mirroring(), Mirroring::Vertical);
        assert_eq!(mapper.cpu_read(0x8000), 0x00);
        // $C000 maps to bank 1 start
        assert_eq!(mapper.cpu_read(0xC000), 0x00); // offset 0x4000 & 0xFF = 0
    }

    #[test]
    fn chr_read_write_ram() {
        let data = make_ines(1, 0, 0x00); // CHR RAM (0 banks)
        let mut mapper = parse_ines(&data).expect("parse failed").mapper;
        assert_eq!(mapper.chr_read(0x0000), 0);
        mapper.chr_write(0x0000, 0xAB);
        assert_eq!(mapper.chr_read(0x0000), 0xAB);
    }

    #[test]
    fn chr_rom_not_writable() {
        let data = make_ines(1, 1, 0x00); // CHR ROM (1 bank)
        let mut mapper = parse_ines(&data).expect("parse failed").mapper;
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
        data[6] = 0xF0; // Mapper 15 (unsupported)
        assert!(parse_ines(&data).is_err());
    }

    // --- NES 2.0 header tests ---

    /// Build a NES 2.0 header with extended PRG/CHR sizes.
    fn make_nes2(prg_banks: u16, chr_banks: u16, mapper: u16) -> Vec<u8> {
        let prg_lo = (prg_banks & 0xFF) as u8;
        let prg_hi = ((prg_banks >> 8) & 0x0F) as u8;
        let chr_lo = (chr_banks & 0xFF) as u8;
        let chr_hi = ((chr_banks >> 8) & 0x0F) as u8;

        let mapper_lo = (mapper & 0x0F) as u8;
        let mapper_mid = ((mapper >> 4) & 0x0F) as u8;
        let mapper_hi = ((mapper >> 8) & 0x0F) as u8;

        let flags6 = mapper_lo << 4; // mapper low nibble in high bits
        let flags7 = (mapper_mid << 4) | 0x08; // mapper mid nibble + NES 2.0 signature
        let byte8 = mapper_hi | 0x00; // mapper high nibble in low bits

        let prg_size = prg_banks as usize * 16384;
        let chr_size = chr_banks as usize * 8192;

        let mut data = vec![0u8; 16 + prg_size + chr_size];
        data[0..4].copy_from_slice(b"NES\x1a");
        data[4] = prg_lo;
        data[5] = chr_lo;
        data[6] = flags6;
        data[7] = flags7;
        data[8] = byte8;
        data[9] = (chr_hi << 4) | prg_hi;

        // Fill PRG with pattern
        for i in 0..prg_size {
            data[16 + i] = (i & 0xFF) as u8;
        }
        // Fill CHR with pattern
        for i in 0..chr_size {
            data[16 + prg_size + i] = ((i + 0x80) & 0xFF) as u8;
        }
        data
    }

    #[test]
    fn nes2_detected_and_parsed() {
        // 2 PRG banks, 1 CHR bank, mapper 0 — NES 2.0 format
        let data = make_nes2(2, 1, 0);
        let result = parse_ines(&data).expect("NES 2.0 parse failed");
        // Should parse as NROM with correct sizes
        assert_eq!(result.mapper.cpu_read(0x8000), 0x00);
    }

    #[test]
    fn nes2_extended_prg_size() {
        // 256 + 2 = 258 PRG banks (prg_hi = 1, prg_lo = 2)
        // This is a huge ROM (4,128 KB) but tests the extended size logic
        let data = make_nes2(258, 1, 0);
        // Should parse without error — 258 * 16384 = 4,227,072 bytes PRG
        let result = parse_ines(&data);
        assert!(result.is_ok(), "Extended PRG size should parse");
    }

    #[test]
    fn nes2_mapper_number_12bit() {
        // Mapper 256 — beyond 8-bit range, won't match any supported mapper
        let data = make_nes2(1, 1, 256);
        let result = parse_ines(&data);
        assert!(result.is_err(), "Mapper 256 should be unsupported");
        let err = result.err().unwrap();
        assert!(
            err.contains("256"),
            "Should report mapper 256 as unsupported: {err}"
        );
    }

    #[test]
    fn ines1_still_works_after_nes2_support() {
        // Standard iNES 1.0 — flags7 bits 3-2 should NOT be 0b10
        let data = make_ines(2, 1, 0x01); // Vertical mirroring, mapper 0
        let result = parse_ines(&data).expect("iNES 1.0 should still parse");
        assert_eq!(result.mapper.mirroring(), Mirroring::Vertical);
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
        let mapper = parse_ines(&data).expect("parse failed").mapper;
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
    fn mmc1_prg_ram_trait() {
        let mut m = make_mmc1(2, 0);
        // prg_ram() should return zeroed 8K
        let ram = m.prg_ram().expect("MMC1 should have PRG RAM");
        assert_eq!(ram.len(), 8192);
        assert!(ram.iter().all(|&b| b == 0));
        // Write some data via cpu_write, then read back through trait
        m.cpu_write(0x6000, 0x42);
        m.cpu_write(0x7FFF, 0xAB);
        let ram = m.prg_ram().expect("MMC1 PRG RAM");
        assert_eq!(ram[0], 0x42);
        assert_eq!(ram[8191], 0xAB);
        // set_prg_ram() restores data
        let mut save = vec![0u8; 8192];
        save[100] = 0xCC;
        m.set_prg_ram(&save);
        assert_eq!(m.cpu_read(0x6000 + 100), 0xCC);
    }

    #[test]
    fn nrom_prg_ram_none() {
        let m = Nrom::new(vec![0u8; 16384], vec![0u8; 8192], Mirroring::Horizontal);
        assert!(m.prg_ram().is_none());
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
        let mapper = parse_ines(&data).expect("parse failed").mapper;
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
        let mapper = parse_ines(&data).expect("parse failed").mapper;
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
        let mapper = parse_ines(&data).expect("parse failed").mapper;
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
        let mapper = parse_ines(&data).expect("parse failed").mapper;
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

        assert_eq!(m.cpu_read(0x8000), 5); // R6
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
        m.cpu_write(0x8001, 5); // R6 = 5

        m.cpu_write(0x8000, 0x47); // reg 7
        m.cpu_write(0x8001, 10); // R7 = 10

        assert_eq!(m.cpu_read(0x8000), 30); // second-to-last (fixed)
        assert_eq!(m.cpu_read(0xA000), 10); // R7
        assert_eq!(m.cpu_read(0xC000), 5); // R6 (swapped to $C000)
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

        m.cpu_write(0x8000, 2);
        m.cpu_write(0x8001, 20); // R2 = 20
        m.cpu_write(0x8000, 3);
        m.cpu_write(0x8001, 21); // R3 = 21
        m.cpu_write(0x8000, 4);
        m.cpu_write(0x8001, 22); // R4 = 22
        m.cpu_write(0x8000, 5);
        m.cpu_write(0x8001, 23); // R5 = 23

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
        m.cpu_write(0x8001, 4); // R0 = 4

        m.cpu_write(0x8000, 0x81); // reg 1
        m.cpu_write(0x8001, 8); // R1 = 8

        m.cpu_write(0x8000, 0x82);
        m.cpu_write(0x8001, 20); // R2
        m.cpu_write(0x8000, 0x83);
        m.cpu_write(0x8001, 21); // R3
        m.cpu_write(0x8000, 0x84);
        m.cpu_write(0x8001, 22); // R4
        m.cpu_write(0x8000, 0x85);
        m.cpu_write(0x8001, 23); // R5

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
        m.cpu_write(0xC000, 3); // latch = 3
        m.cpu_write(0xC001, 0); // reload flag set

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
        let mapper = parse_ines(&data).expect("parse failed").mapper;
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
        prg[0] = 0xAA; // bank 0, addr $8000
        prg[32768] = 0xBB; // bank 1, addr $8000
        prg[2 * 32768] = 0xCC; // bank 2, addr $8000
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
        chr[0] = 0x11; // bank 0
        chr[8192] = 0x22; // bank 1
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
        prg[0] = 0xAA; // bank 0
        prg[32768] = 0xBB; // bank 1
        prg[2 * 32768] = 0xCC; // bank 2
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
        prg[0] = 0xAA; // bank 0
        prg[16384] = 0xBB; // bank 1
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
        chr[0] = 0x11; // bank 0 ($FD)
        chr[4096] = 0x22; // bank 1 ($FE)
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
        prg[0] = 0xFF; // bus-conflict-safe write target
        prg[1] = 0xAA; // identifying byte at offset 1
        prg[32768] = 0xFF; // bank 1 write target
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

    // --- Mapper 87 tests ---

    #[test]
    fn mapper87_parse_ines() {
        // Mapper 87 = 0x57 → flags6 high nibble = 0x70, flags7 high nibble = 0x50
        let mut data = make_ines(1, 2, 0x70);
        data[7] = 0x50; // mapper high nibble
        let m = parse_ines(&data);
        assert!(m.is_ok(), "Mapper 87 should parse successfully");
    }

    #[test]
    fn mapper87_chr_bank_swap() {
        // 2 × 8K CHR banks: bank 0 filled with 0, bank 1 filled with 1
        let mut chr = vec![0u8; 16384];
        for b in &mut chr[8192..] {
            *b = 1;
        }
        let mut m = Mapper87::new(vec![0u8; 16384], chr, Mirroring::Horizontal);

        // Default: bank 0
        assert_eq!(m.chr_read(0x0000), 0);

        // Write 0x01 → swapped: bank = (0<<0)|(1<<1) = 0 — wait, let's think:
        // Written value 0x01: bit0=1, bit1=0.
        // Swapped: (bit0<<1)|(bit1>>1) = (1<<1)|(0) = 2
        m.cpu_write(0x6000, 0x01);
        // chr_bank = ((1)<<1) | ((0>>1)&1) = 2
        // With 2 banks, bank 2 % 2 = bank 0 → still bank 0
        // Let's use write 0x02 instead: bit0=0, bit1=1 → swap: (0<<1)|(1) = 1
        m.cpu_write(0x6000, 0x02);
        assert_eq!(m.chr_read(0x0000), 1); // bank 1

        // Write 0x00 → bank 0
        m.cpu_write(0x6000, 0x00);
        assert_eq!(m.chr_read(0x0000), 0);
    }

    #[test]
    fn mapper87_prg_unbanked() {
        let mut prg = vec![0u8; 16384];
        prg[0] = 0xAA;
        let m = Mapper87::new(prg, vec![0u8; 8192], Mirroring::Vertical);
        assert_eq!(m.cpu_read(0x8000), 0xAA);
        // 16K mirrored
        assert_eq!(m.cpu_read(0xC000), 0xAA);
    }

    // --- Mapper 206 tests ---

    #[test]
    fn mapper206_parse_ines() {
        // Mapper 206 = 0xCE → flags6 high nibble = 0xE0, flags7 high nibble = 0xC0
        let mut data = make_ines(4, 4, 0xE0);
        data[7] = 0xC0;
        let m = parse_ines(&data);
        assert!(m.is_ok(), "Mapper 206 should parse successfully");
    }

    #[test]
    fn mapper206_prg_banking() {
        // 4 × 8K PRG banks, each filled with bank number
        let mut prg = vec![0u8; 32768];
        for bank in 0..4usize {
            for i in 0..8192 {
                prg[bank * 8192 + i] = bank as u8;
            }
        }
        let mut m = Mapper206::new(prg, vec![0u8; 32768], Mirroring::Vertical);

        // Default: R6=0, R7=0. $C000=bank 2, $E000=bank 3 (fixed).
        assert_eq!(m.cpu_read(0x8000), 0); // R6=0 → bank 0
        assert_eq!(m.cpu_read(0xC000), 2); // second-to-last
        assert_eq!(m.cpu_read(0xE000), 3); // last

        // Select R6=1 (bank 1 at $8000)
        m.cpu_write(0x8000, 6); // bank_select = 6 → target R6
        m.cpu_write(0x8001, 1); // R6 = 1
        assert_eq!(m.cpu_read(0x8000), 1);

        // Select R7=2 (bank 2 at $A000)
        m.cpu_write(0x8000, 7); // target R7
        m.cpu_write(0x8001, 2); // R7 = 2
        assert_eq!(m.cpu_read(0xA000), 2);
    }

    #[test]
    fn mapper206_no_irq() {
        let m = Mapper206::new(vec![0u8; 32768], vec![0u8; 8192], Mirroring::Horizontal);
        assert!(!m.irq_pending());
    }

    #[test]
    fn mapper206_fixed_mirroring() {
        let mut m = Mapper206::new(vec![0u8; 32768], vec![0u8; 8192], Mirroring::Horizontal);
        // Write to $A000 (mirroring control on MMC3) should be ignored
        m.cpu_write(0xA000, 0x01);
        assert_eq!(m.mirroring(), Mirroring::Horizontal);
    }

    #[test]
    fn mapper206_chr_banking() {
        // 32K CHR: 32 × 1K pages, each filled with page index
        let mut chr = vec![0u8; 32768];
        for page in 0..32usize {
            for i in 0..1024 {
                chr[page * 1024 + i] = page as u8;
            }
        }
        let mut m = Mapper206::new(vec![0u8; 32768], chr, Mirroring::Vertical);

        // Mode 0 (default): R0 selects 2K at $0000, R2 selects 1K at $1000
        m.cpu_write(0x8000, 0); // target R0
        m.cpu_write(0x8001, 4); // R0=4 → 2K-aligned → pages 4,5
        assert_eq!(m.chr_read(0x0000), 4); // page 4
        assert_eq!(m.chr_read(0x0400), 5); // page 5

        m.cpu_write(0x8000, 2); // target R2
        m.cpu_write(0x8001, 10); // R2=10 → 1K page 10 at $1000
        assert_eq!(m.chr_read(0x1000), 10);
    }
}
